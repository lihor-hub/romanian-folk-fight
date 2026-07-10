//! `cargo xtask assets review` -- a deterministic, self-contained static
//! HTML gallery generated entirely from the #167/#185 sidecar aggregate
//! (#197, a child of #141). No hand-maintained manifest, no metadata
//! duplicated from `src/cutout.rs`/`src/items/visuals.rs` beyond the two
//! documented point-in-time snapshots this module's submodules already
//! declare (`model::DRAW_ORDER`, `pages::PANEL_BORDER_INSET_PX`) -- the same
//! documented-snapshot convention #167/#185 already use for `pivot`/
//! `display`.
//!
//! # Page types
//!
//! - **Fighter/gear rig-attachment parts** (`fighter-runtime-part`,
//!   `gear-runtime-part`, and the one `gear-overlay` still `status =
//!   runtime`): source-sheet crop context, the runtime image at real game
//!   scale over a checkerboard and a representative background, a mirrored
//!   review-aid preview, and a rig-space pivot/attachment diagram. See
//!   `pages::render_part_page`.
//! - **Compositions**: one page per fighter identity (`human`, `strigoi`,
//!   `zmeu`, discovered from the aggregate, not hardcoded) assembling every
//!   `fighter-runtime-part` at its sidecar `pivot`/`display`, and one page
//!   per composable gear item showing it equipped onto the human rig. Both
//!   facings are rendered on one shared canvas (see `layout.rs`'s
//!   coordinate-convention doc comment for exactly what is and isn't
//!   reproduced).
//! - **UI**: an icon page (native size + 4x zoom + panel-toned backdrop) and
//!   a 9-slice panel-border preview at representative sizes over a
//!   linen-toned backdrop.
//! - **Backgrounds**: one page per parallax scene (grouped by id, e.g.
//!   `village`) compositing its far/near/foreground layers, plus one plain
//!   page per layer record linking back to its scene.
//! - **Fonts/documents/audio**: metadata-only pages (family/metrics via
//!   `probe.rs`, best-effort; a native `<audio>` element for convenience) --
//!   never a fake raster preview.
//! - **Everything else** (source sheets, legacy gear overlays, placeholder
//!   sprites, web icons/images): a plain preview + metadata page.
//!
//! # Determinism
//!
//! Every collection this module iterates is sorted (by id, or by the
//! documented anatomical draw order) before it is written; nothing here
//! reads the clock, a random source, or filesystem iteration order (see
//! `discover::walk_assets`, already sorted). `generate` removes and
//! recreates `out_dir` on every run, so two clean runs against the same
//! `assets/` tree produce byte-identical files -- see the
//! `two_clean_runs_produce_byte_identical_output` test below.

pub mod layout;
pub mod model;
pub mod pages;
pub mod probe;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::aggregate::{self, ResolvedRecord};
use super::schema::{Category, Kind};
use layout::PartPlacement;
use pages::CompositionLayer;

#[derive(Debug)]
pub enum GalleryError {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl fmt::Display for GalleryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GalleryError::Io { path, error } => {
                write!(f, "{}: {error}", path.display())
            }
        }
    }
}

pub struct GalleryReport {
    pub index_path: PathBuf,
    pub page_count: usize,
}

fn write_page(out_dir: &Path, id: &str, html: &str) -> Result<(), GalleryError> {
    let path = out_dir.join(format!("{id}.html"));
    fs::write(&path, html).map_err(|error| GalleryError::Io { path, error })
}

/// Generates the full gallery into `out_dir` (removed and recreated first),
/// deriving every page from the sidecar aggregate rooted at `assets_root`.
pub fn generate(assets_root: &Path, out_dir: &Path) -> Result<GalleryReport, GalleryError> {
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).map_err(|error| GalleryError::Io {
            path: out_dir.to_path_buf(),
            error,
        })?;
    }
    fs::create_dir_all(out_dir).map_err(|error| GalleryError::Io {
        path: out_dir.to_path_buf(),
        error,
    })?;

    let built = aggregate::build(assets_root);
    let mut records: Vec<&ResolvedRecord> = built.records.iter().collect();
    records.sort_by(|a, b| a.record.id.cmp(&b.record.id));
    let by_id: BTreeMap<&str, &ResolvedRecord> =
        records.iter().map(|r| (r.record.id.as_str(), *r)).collect();

    let mut page_count = 0usize;
    let mut index = IndexBuilder::default();

    // --- Fighter identity compositions + parts ---
    let identities = model::fighter_identities(&records);
    for identity in &identities {
        let parts = model::identity_parts(&records, identity);
        let layers: Vec<CompositionLayer> = parts
            .iter()
            .map(|r| CompositionLayer {
                record: r,
                placement: PartPlacement {
                    pivot: r.record.pivot.unwrap_or([0.0, 0.0]),
                    display: r.record.display.unwrap_or([1.0, 1.0]),
                },
                z: model::draw_order_index(r.record.attachment.as_deref().unwrap_or("")),
            })
            .collect();
        let comp_id = format!("composition.{identity}");
        let title = format!("{identity} (neutral pose)");
        let note = "Assembled from this identity's fighter-runtime-part records: each part translated to its \
                     sidecar pivot and sized to its display, in the documented anatomical draw order. Rotation \
                     is not tracked in any sidecar field, so this is an unrotated approximation of the rest \
                     pose -- see layout.rs's module docs.";
        write_page(
            out_dir,
            &comp_id,
            &pages::render_composition_page(&title, &comp_id, &layers, note),
        )?;
        page_count += 1;
        index.push("Fighter compositions", &comp_id, &comp_id);

        for part in &parts {
            let composition_links =
                vec![(format!("{identity} composition"), format!("{comp_id}.html"))];
            let source_sheet = part
                .record
                .source_sheet
                .as_deref()
                .and_then(|id| by_id.get(id))
                .copied();
            let html = pages::render_part_page(
                part,
                source_sheet,
                &representative_background_href(&by_id),
                &composition_links,
            );
            write_page(out_dir, &part.record.id, &html)?;
            page_count += 1;
            index.push(
                &format!("Fighter parts: {identity}"),
                &part.record.id,
                &part.record.id,
            );
        }
    }

    // --- Gear: composable (rig-attached) items get a composition + part page ---
    let human_parts = model::identity_parts(&records, "human");
    let human_by_attachment: BTreeMap<&str, PartPlacement> = human_parts
        .iter()
        .filter_map(|p| {
            let attachment = p.record.attachment.as_deref()?;
            Some((
                attachment,
                PartPlacement {
                    pivot: p.record.pivot.unwrap_or([0.0, 0.0]),
                    display: p.record.display.unwrap_or([1.0, 1.0]),
                },
            ))
        })
        .collect();

    for gear in model::composable_gear(&records) {
        let gear_attachment = gear.record.attachment.as_deref().unwrap_or("");
        let gear_pivot = gear.record.pivot.unwrap_or([0.0, 0.0]);
        let gear_display = gear.record.display.unwrap_or([1.0, 1.0]);

        let mut layers: Vec<CompositionLayer> = human_parts
            .iter()
            .map(|p| CompositionLayer {
                record: p,
                placement: PartPlacement {
                    pivot: p.record.pivot.unwrap_or([0.0, 0.0]),
                    display: p.record.display.unwrap_or([1.0, 1.0]),
                },
                z: model::draw_order_index(p.record.attachment.as_deref().unwrap_or("")),
            })
            .collect();

        for attachment_part in model::attachment_parts(gear_attachment) {
            let Some(part_placement) = human_by_attachment.get(attachment_part) else {
                continue;
            };
            let world_pivot = [
                part_placement.pivot[0] + gear_pivot[0],
                part_placement.pivot[1] + gear_pivot[1],
            ];
            layers.push(CompositionLayer {
                record: gear,
                placement: PartPlacement {
                    pivot: world_pivot,
                    display: gear_display,
                },
                // Gear draws immediately after its own attachment part, biased by
                // a half-step so it never collides with the next anatomical part
                // in the fixed sort key (both are `usize`, so encode the bias by
                // sorting gear after parts sharing the same attachment index --
                // stable because `Vec::sort_by_key` is stable and gear layers are
                // pushed after every body-part layer above).
                z: model::draw_order_index(attachment_part),
            });
        }

        let slug = model::last_segment(&gear.record.id);
        let comp_id = format!("composition.gear.{slug}");
        let title = format!("Human + {slug} (equipped)");
        let note = "Composed onto the human base rig: the gear's sidecar pivot is added to its attachment \
                     part's own pivot (both translation-only, matching src/cutout.rs's parent-child spawn \
                     relationship with rest-pose rotation omitted -- see layout.rs). The same attachment \
                     point exists on the strigoi/zmeu identities using their own pivots; this page shows the \
                     human rig as the representative composition.";
        write_page(
            out_dir,
            &comp_id,
            &pages::render_composition_page(&title, &comp_id, &layers, note),
        )?;
        page_count += 1;
        index.push("Gear compositions", &comp_id, &comp_id);

        let composition_links = vec![(
            format!("Human + {slug} composition"),
            format!("{comp_id}.html"),
        )];
        let source_sheet = gear
            .record
            .source_sheet
            .as_deref()
            .and_then(|id| by_id.get(id))
            .copied();
        let html = pages::render_part_page(
            gear,
            source_sheet,
            &representative_background_href(&by_id),
            &composition_links,
        );
        write_page(out_dir, &gear.record.id, &html)?;
        page_count += 1;
        index.push("Gear parts", &gear.record.id, &gear.record.id);
    }

    // --- Gear without rig metadata (legacy overlays) ---
    for record in &records {
        if record.record.category != Category::GearOverlay {
            continue;
        }
        if model::composable_gear(&records)
            .iter()
            .any(|g| g.record.id == record.record.id)
        {
            continue;
        }
        let html = pages::render_generic_asset_page(record, &[]);
        write_page(out_dir, &record.record.id, &html)?;
        page_count += 1;
        index.push(
            "Gear (legacy, unreferenced)",
            &record.record.id,
            &record.record.id,
        );
    }

    // --- UI ---
    for record in &records {
        match record.record.category {
            Category::UiIcon => {
                let html = pages::render_ui_icon_page(record);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push("UI icons", &record.record.id, &record.record.id);
            }
            Category::UiPanel => {
                let html = pages::render_ui_panel_page(record);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push("UI panels", &record.record.id, &record.record.id);
            }
            Category::UiSourceSheet => {
                let html = pages::render_generic_asset_page(record, &[]);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push("UI", &record.record.id, &record.record.id);
            }
            _ => {}
        }
    }

    // --- Backgrounds ---
    let scenes = model::background_scenes(&records);
    for scene in &scenes {
        let scene_id = format!("composition.background.{}", scene.scene);
        let html = pages::render_background_scene_page(&scene.scene, &scene.layers);
        write_page(out_dir, &scene_id, &html)?;
        page_count += 1;
        index.push("Background scenes", &scene_id, &scene_id);

        let related = vec![(
            format!("{} scene composition", scene.scene),
            format!("{scene_id}.html"),
        )];
        for layer in &scene.layers {
            let html = pages::render_generic_asset_page(layer, &related);
            write_page(out_dir, &layer.record.id, &html)?;
            page_count += 1;
            index.push("Background layers", &layer.record.id, &layer.record.id);
        }
    }

    // --- Sprites (legacy placeholders), source sheets, web assets ---
    for record in &records {
        let section = match record.record.category {
            Category::Sprite => Some("Sprites (bootstrap placeholders)"),
            Category::FighterSourceSheet | Category::GearSourceSheet => Some("Source sheets"),
            Category::WebIcon | Category::WebImage => Some("Web"),
            _ => None,
        };
        if let Some(section) = section {
            let html = pages::render_generic_asset_page(record, &[]);
            write_page(out_dir, &record.record.id, &html)?;
            page_count += 1;
            index.push(section, &record.record.id, &record.record.id);
        }
    }

    // --- Fonts, font licenses, audio ---
    for record in &records {
        match (record.record.kind, record.record.category) {
            (Kind::Font, _) => {
                let bytes = fs::read(assets_root.join(&record.full_path)).unwrap_or_default();
                let probe = probe::probe_font(&bytes);
                let html = pages::render_font_page(record, &probe);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push("Fonts", &record.record.id, &record.record.id);
            }
            (Kind::Document, Category::FontLicense) => {
                let html = pages::render_document_page(record);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push("Fonts", &record.record.id, &record.record.id);
            }
            (Kind::Audio, _) => {
                let bytes = fs::read(assets_root.join(&record.full_path)).unwrap_or_default();
                let probe = probe::probe_ogg(&bytes);
                let html = pages::render_audio_page(record, &probe);
                write_page(out_dir, &record.record.id, &html)?;
                page_count += 1;
                index.push(
                    section_for_audio(record),
                    &record.record.id,
                    &record.record.id,
                );
            }
            _ => {}
        }
    }

    let index_html = index.render(&records, &built.diagnostics);
    let index_path = out_dir.join("index.html");
    fs::write(&index_path, index_html).map_err(|error| GalleryError::Io {
        path: index_path.clone(),
        error,
    })?;

    Ok(GalleryReport {
        index_path,
        page_count,
    })
}

fn section_for_audio(record: &ResolvedRecord) -> &'static str {
    match record.record.category {
        Category::Music => "Audio: music",
        Category::Sting => "Audio: stings",
        _ => "Audio: sfx",
    }
}

fn representative_background_href(by_id: &BTreeMap<&str, &ResolvedRecord>) -> String {
    by_id
        .get(pages::REPRESENTATIVE_BACKGROUND_ID)
        .map(|r| pages::asset_href_resolved(r))
        .unwrap_or_else(|| format!("{}/backgrounds/village_near.png", pages::ASSET_REL))
}

/// Accumulates `(section, id, href-without-extension)` entries in insertion
/// order (already deterministic: every loop above iterates sorted `records`
/// or a sorted derived collection), grouped by section for the index page.
#[derive(Default)]
struct IndexBuilder {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl IndexBuilder {
    fn push(&mut self, section: &str, id: &str, href_stem: &str) {
        if let Some(existing) = self.sections.iter_mut().find(|(name, _)| name == section) {
            existing
                .1
                .push((id.to_string(), format!("{href_stem}.html")));
        } else {
            self.sections.push((
                section.to_string(),
                vec![(id.to_string(), format!("{href_stem}.html"))],
            ));
        }
    }

    fn render(
        &self,
        records: &[&ResolvedRecord],
        diagnostics: &[super::diagnostics::Diagnostic],
    ) -> String {
        let mut body = String::from("<h1>Asset review gallery</h1>\n");
        body.push_str(&format!(
            "<p class=\"caption\">{} record(s) from the sidecar aggregate. \
             {} sidecar diagnostic(s) (see <code>cargo xtask assets check</code> for detail).</p>\n",
            records.len(),
            diagnostics.len()
        ));
        for (section, entries) in &self.sections {
            body.push_str(&format!(
                "<section>\n<h2>{}</h2>\n<ul>\n",
                pages::escape_html(section)
            ));
            for (id, href) in entries {
                body.push_str(&format!(
                    "<li><a href=\"{href}\">{}</a></li>\n",
                    pages::escape_html(id)
                ));
            }
            body.push_str("</ul>\n</section>\n");
        }
        // The index has no "back to index" breadcrumb target other than itself.
        let shell = pages::page_shell("Asset review gallery", &body);
        shell.replace(
            "<p class=\"breadcrumb\"><a href=\"index.html\">&larr; Asset gallery index</a></p>\n",
            "",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempOut {
        root: PathBuf,
    }

    impl TempOut {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "xtask-gallery-{name}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::remove_dir_all(&root);
            Self { root }
        }
    }

    impl Drop for TempOut {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn real_assets_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask/Cargo.toml always has a parent workspace root")
            .join("assets")
    }

    #[test]
    fn generate_produces_an_index_and_representative_page_types() {
        let assets_root = real_assets_root();
        let out = TempOut::new("page-types");
        let report = generate(&assets_root, &out.root)
            .expect("generate succeeds against the real asset tree");
        assert!(report.index_path.exists());
        assert!(report.page_count > 0);

        // Fighter part page.
        assert!(out.root.join("fighters.human.runtime.head.html").exists());
        // Fighter composition page.
        assert!(out.root.join("composition.human.html").exists());
        // Gear composition page.
        assert!(out.root.join("composition.gear.palos.html").exists());
        // UI panel page.
        assert!(out.root.join("ui.panel-border.html").exists());
        // Background scene composition page.
        assert!(
            out.root
                .join("composition.background.village.html")
                .exists()
        );
        // Font metadata page.
        assert!(out.root.join("fonts.alegreya-variable.html").exists());
        // Audio metadata page.
        assert!(out.root.join("audio.music-menu.html").exists());
    }

    #[test]
    fn fighter_part_page_structure_has_source_crop_scale_and_pivot_sections() {
        let assets_root = real_assets_root();
        let out = TempOut::new("fighter-part");
        generate(&assets_root, &out.root).unwrap();
        let html = fs::read_to_string(out.root.join("fighters.human.runtime.head.html")).unwrap();
        assert!(html.contains("Source crop"));
        assert!(html.contains("Runtime image at real game scale"));
        assert!(html.contains("Pivot / attachment guide"));
        assert!(html.contains("composition.human.html"));
    }

    #[test]
    fn gear_part_page_links_to_its_equipped_composition() {
        let assets_root = real_assets_root();
        let out = TempOut::new("gear-part");
        generate(&assets_root, &out.root).unwrap();
        let html = fs::read_to_string(out.root.join("fighters.gear.runtime.palos.html")).unwrap();
        assert!(html.contains("composition.gear.palos.html"));
    }

    #[test]
    fn ui_panel_page_renders_representative_sizes() {
        let assets_root = real_assets_root();
        let out = TempOut::new("ui-panel");
        generate(&assets_root, &out.root).unwrap();
        let html = fs::read_to_string(out.root.join("ui.panel-border.html")).unwrap();
        for (w, h) in pages::REPRESENTATIVE_PANEL_SIZES {
            assert!(html.contains(&format!("{w}px")));
            assert!(html.contains(&format!("{h}px")));
        }
    }

    #[test]
    fn background_scene_page_lists_all_three_layers_in_order() {
        let assets_root = real_assets_root();
        let out = TempOut::new("background");
        generate(&assets_root, &out.root).unwrap();
        let html =
            fs::read_to_string(out.root.join("composition.background.village.html")).unwrap();
        let far = html.find("backgrounds.village-far").unwrap();
        let near = html.find("backgrounds.village-near").unwrap();
        let fg = html.find("backgrounds.village-foreground").unwrap();
        assert!(
            far < near && near < fg,
            "layers must be listed far < near < foreground"
        );
    }

    #[test]
    fn font_page_has_no_raster_preview_but_shows_metrics() {
        let assets_root = real_assets_root();
        let out = TempOut::new("font");
        generate(&assets_root, &out.root).unwrap();
        let html = fs::read_to_string(out.root.join("fonts.alegreya-variable.html")).unwrap();
        assert!(html.contains("Font metrics"));
        assert!(
            !html.contains("<img"),
            "a font page must never contain a raster preview"
        );
    }

    #[test]
    fn audio_page_has_no_raster_preview_but_shows_a_native_audio_element() {
        let assets_root = real_assets_root();
        let out = TempOut::new("audio");
        generate(&assets_root, &out.root).unwrap();
        let html = fs::read_to_string(out.root.join("audio.music-menu.html")).unwrap();
        assert!(html.contains("<audio"));
        assert!(
            !html.contains("<img"),
            "an audio page must never contain a raster preview"
        );
    }

    #[test]
    fn two_clean_runs_produce_byte_identical_output() {
        let assets_root = real_assets_root();
        let out_a = TempOut::new("determinism-a");
        let out_b = TempOut::new("determinism-b");
        generate(&assets_root, &out_a.root).unwrap();
        generate(&assets_root, &out_b.root).unwrap();

        let mut files_a: Vec<PathBuf> = fs::read_dir(&out_a.root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into())
            .collect();
        let mut files_b: Vec<PathBuf> = fs::read_dir(&out_b.root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into())
            .collect();
        files_a.sort();
        files_b.sort();
        assert_eq!(
            files_a, files_b,
            "both runs must produce the exact same set of files"
        );

        for file in &files_a {
            let bytes_a = fs::read(out_a.root.join(file)).unwrap();
            let bytes_b = fs::read(out_b.root.join(file)).unwrap();
            assert_eq!(
                bytes_a,
                bytes_b,
                "{} differs between two clean runs",
                file.display()
            );
        }
    }

    #[test]
    fn regenerating_into_the_same_directory_never_leaves_stale_files() {
        let assets_root = real_assets_root();
        let out = TempOut::new("regenerate");
        generate(&assets_root, &out.root).unwrap();
        fs::write(out.root.join("stale-leftover.html"), "old content").unwrap();
        generate(&assets_root, &out.root).unwrap();
        assert!(
            !out.root.join("stale-leftover.html").exists(),
            "generate must wipe out_dir before writing"
        );
    }
}
