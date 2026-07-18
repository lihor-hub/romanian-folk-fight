//! HTML page rendering for the review gallery (#197). Every function here is
//! a pure string builder: given already-resolved sidecar data (never
//! hand-maintained, never duplicated numbers), it returns a complete,
//! self-contained HTML document (inline `<style>`, relative `<img>` links
//! back into `assets/`, no external CDNs). Determinism follows from purity:
//! the same input records always produce the same output bytes.

use super::super::aggregate::ResolvedRecord;
use super::super::schema::Sampler;
use super::layout::{Box2D, PartPlacement, RigCanvas};

/// Every generated page lives directly in the gallery output directory, so
/// every page (including the index) is the same number of directories below
/// `assets/`: `target/xtask-artifacts/asset-gallery/<page>.html` ->
/// `../../../assets/...`.
pub const ASSET_REL: &str = "../../../assets";

/// The one fixed background asset used as "a representative background"
/// behind every fighter/gear part and composition preview (see module docs
/// in `mod.rs` for why a real game asset is used here instead of an
/// invented placeholder).
pub const REPRESENTATIVE_BACKGROUND_ID: &str = "backgrounds.village-near";

/// Pixel inset of `ui/panel_border.png`'s 9-slice border. A point-in-time
/// snapshot of `PANEL_BORDER_INSET` in `src/theme/mod.rs` (same documented
/// snapshot risk #167/#185 already accept for rig `pivot`/`display` --
/// see `xtask/README.md`'s "Known limitations").
pub const PANEL_BORDER_INSET_PX: u32 = 24;

/// Representative panel sizes (px) shown on the UI panel's 9-slice preview
/// page, spanning the shop/HUD panel range this texture is actually used at.
pub const REPRESENTATIVE_PANEL_SIZES: &[(u32, u32)] = &[(180, 120), (320, 200), (480, 320)];

/// Synthetic "linen" backdrop color for panel/UI context previews -- the
/// closest documented palette color (`docs/art-direction.md`'s Cream,
/// `#e8dcc8`) since no literal linen-texture asset exists in this repo.
pub const LINEN_BACKDROP: &str = "#e8dcc8";

pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Path of a resolved record's own runtime file, relative to a page in the
/// gallery output root. This is the one every page actually uses.
pub fn asset_href_resolved(record: &ResolvedRecord) -> String {
    let full = record.full_path.to_string_lossy().replace('\\', "/");
    format!("{ASSET_REL}/{full}")
}

const STYLE: &str = r#"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body {
  font-family: -apple-system, "Segoe UI", Roboto, sans-serif;
  margin: 0;
  padding: 24px 32px 64px;
  background: #1a1214;
  color: #e8dcc8;
  line-height: 1.4;
}
a { color: #c9a227; }
a:visited { color: #e0b94a; }
h1 { margin-top: 0; }
h2 { border-bottom: 1px solid #7a1f1f; padding-bottom: 4px; }
.breadcrumb { margin-bottom: 16px; opacity: 0.85; }
table.meta { border-collapse: collapse; margin: 12px 0 28px; }
table.meta td, table.meta th {
  border: 1px solid rgba(232, 220, 200, 0.25);
  padding: 4px 12px;
  text-align: left;
  font-size: 14px;
  vertical-align: top;
}
table.meta th { color: #c9a227; white-space: nowrap; }
section { margin-bottom: 36px; }
.swatch-row, .nine-slice-row { display: flex; gap: 20px; flex-wrap: wrap; align-items: flex-end; }
.swatch {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
  border: 1px solid rgba(232, 220, 200, 0.3);
  min-width: 48px;
  min-height: 48px;
}
.swatch-label { font-size: 12px; opacity: 0.75; margin: 6px 0 0; }
.checkerboard {
  background-image:
    linear-gradient(45deg, #9a9a9a 25%, transparent 25%),
    linear-gradient(-45deg, #9a9a9a 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, #9a9a9a 75%),
    linear-gradient(-45deg, transparent 75%, #9a9a9a 75%);
  background-size: 16px 16px;
  background-position: 0 0, 0 8px, 8px -8px, -8px 0px;
  background-color: #cfcfcf;
}
.representative-bg { background-size: cover; background-position: center; }
.pixelated {
  image-rendering: pixelated;
  image-rendering: -moz-crisp-edges;
  image-rendering: crisp-edges;
}
.smooth { image-rendering: auto; }
.mirrored { transform: scaleX(-1); }
.rig-canvas {
  position: relative;
  background-color: rgba(0, 0, 0, 0.2);
  border: 1px dashed rgba(232, 220, 200, 0.3);
}
.rig-canvas img { position: absolute; }
.rig-crosshair { position: absolute; width: 0; height: 0; }
.rig-crosshair::before, .rig-crosshair::after { content: ""; position: absolute; background: #e63946; }
.rig-crosshair::before { left: -6px; top: -1px; width: 12px; height: 2px; }
.rig-crosshair::after { left: -1px; top: -6px; width: 2px; height: 12px; }
.rig-box { position: absolute; border: 1px solid #2ec4b6; box-sizing: border-box; pointer-events: none; }
.rig-source-box { position: absolute; border: 2px solid #e63946; box-sizing: border-box; }
.caption { font-size: 13px; opacity: 0.85; max-width: 680px; }
.nine-slice {
  border-style: solid;
  border-width: 24px;
  border-image-slice: 24 fill;
  border-image-width: 24px;
  border-image-repeat: round;
}
.panel-preview { display: flex; align-items: center; justify-content: center; }
.scene-composite { position: relative; overflow: hidden; }
.scene-composite img { position: absolute; top: 0; left: 0; width: 100%; height: 100%; }
.pill { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 12px; margin-right: 6px; }
.pill-runtime { background: #22432299; }
.pill-source { background: #22314399; }
.pill-legacy { background: #43222299; }
"#;

fn status_pill(status_str: &str) -> String {
    let class = match status_str {
        "runtime" => "pill-runtime",
        "source" => "pill-source",
        _ => "pill-legacy",
    };
    format!("<span class=\"pill {class}\">{status_str}</span>")
}

/// Wraps `body` in a complete, self-contained HTML document.
pub fn page_shell(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>{title}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n\
         <p class=\"breadcrumb\"><a href=\"index.html\">&larr; Asset gallery index</a></p>\n\
         {body}\n</body>\n</html>\n",
        title = escape_html(title),
    )
}

fn meta_row(label: &str, value: impl Into<String>) -> String {
    format!(
        "<tr><th>{}</th><td>{}</td></tr>",
        escape_html(label),
        value.into()
    )
}

/// The metadata table common to every asset page: every field a sidecar
/// record can carry, present or not, so a reviewer never has to open the
/// sidecar file to see the contract.
pub fn metadata_table(record: &ResolvedRecord) -> String {
    let r = &record.record;
    let mut rows = vec![
        meta_row("id", format!("<code>{}</code>", escape_html(&r.id))),
        meta_row("path", escape_html(&record.full_path.to_string_lossy())),
        meta_row("kind", r.kind.to_string()),
        meta_row("category", r.category.to_string()),
        meta_row("status", status_pill(&r.status.to_string())),
        meta_row("provenance", escape_html(&r.provenance)),
        meta_row("license", escape_html(&r.license)),
        meta_row("sidecar", escape_html(&record.sidecar.to_string_lossy())),
    ];
    if let Some(dims) = r.dimensions {
        rows.push(meta_row(
            "dimensions",
            format!("{}&times;{}", dims[0], dims[1]),
        ));
    }
    if let Some(sampler) = r.sampler {
        rows.push(meta_row("sampler", sampler.to_string()));
    }
    if let Some(generator) = &r.generator {
        rows.push(meta_row("generator", escape_html(generator)));
    }
    if let Some(source_sheet) = &r.source_sheet {
        rows.push(meta_row(
            "source_sheet",
            format!(
                "<a href=\"{}.html\">{}</a>",
                escape_html(source_sheet),
                escape_html(source_sheet)
            ),
        ));
    }
    if let Some(license_file) = &r.license_file {
        rows.push(meta_row(
            "license_file",
            format!(
                "<a href=\"{ASSET_REL}/{}\">{}</a>",
                escape_html(license_file),
                escape_html(license_file)
            ),
        ));
    }
    if let Some(attachment) = &r.attachment {
        rows.push(meta_row("attachment", escape_html(attachment)));
    }
    if let Some(pivot) = r.pivot {
        rows.push(meta_row(
            "pivot",
            format!("[{:.2}, {:.2}]", pivot[0], pivot[1]),
        ));
    }
    if let Some(display) = r.display {
        rows.push(meta_row(
            "display",
            format!("[{:.2}, {:.2}]", display[0], display[1]),
        ));
    }
    if let Some(crop) = &r.crop {
        rows.push(meta_row("crop", escape_html(crop)));
    }
    format!("<table class=\"meta\">\n{}\n</table>", rows.join("\n"))
}

fn sampler_class(sampler: Option<Sampler>) -> &'static str {
    match sampler {
        Some(Sampler::Nearest) | None => "pixelated",
        Some(Sampler::Linear) => "smooth",
    }
}

/// Parses a known (non-`"unknown"`) `"x,y,w,h"` crop string into floats.
fn parse_known_crop(crop: &str) -> Option<(f32, f32, f32, f32)> {
    if crop == "unknown" {
        return None;
    }
    let parts: Vec<f32> = crop
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if parts.len() == 4 {
        Some((parts[0], parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

/// Renders one rig-attachment part page (fighter body part, gear runtime
/// part, or runtime gear overlay): source-sheet crop context, the runtime
/// image at real game scale over a checkerboard and a representative
/// background, both facings, and a rig-space pivot/attachment diagram.
#[allow(clippy::too_many_arguments)]
pub fn render_part_page(
    record: &ResolvedRecord,
    source_sheet: Option<&ResolvedRecord>,
    representative_bg_href: &str,
    composition_links: &[(String, String)],
) -> String {
    let r = &record.record;
    let href = asset_href_resolved(record);
    let sampler_cls = sampler_class(r.sampler);
    let display = r.display.unwrap_or([64.0, 64.0]);
    let pivot = r.pivot.unwrap_or([0.0, 0.0]);

    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&r.id),
        metadata_table(record)
    );

    // --- Source crop context ---
    body.push_str("<section>\n<h2>Source crop</h2>\n");
    if let Some(source) = source_sheet {
        let source_href = asset_href_resolved(source);
        let source_dims = source.record.dimensions.unwrap_or([1, 1]);
        let known_crop = r.crop.as_deref().and_then(parse_known_crop);
        body.push_str(&format!(
            "<div class=\"swatch\" style=\"width:360px;height:{}px;\">\n\
             <img src=\"{source_href}\" style=\"width:100%;height:auto;display:block;\" alt=\"source sheet\">\n",
            (360.0 * source_dims[1] as f32 / source_dims[0] as f32).max(1.0)
        ));
        if let Some((x, y, w, h)) = known_crop {
            let scale = 360.0 / source_dims[0] as f32;
            body.push_str(&format!(
                "<div class=\"rig-source-box\" style=\"left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;\"></div>\n",
                x * scale,
                y * scale,
                w * scale,
                h * scale
            ));
        }
        body.push_str("</div>\n");
        if known_crop.is_some() {
            body.push_str("<p class=\"caption\">Highlighted rectangle is this record's known <code>crop</code>, plotted against the full source sheet above.</p>\n");
        } else {
            body.push_str(
                "<p class=\"caption\">This record's <code>crop</code> is the documented \
                 <code>\"unknown\"</code> sentinel (the source-sheet crop rectangle was never \
                 tracked -- see <code>xtask/README.md</code>'s known limitations), so no \
                 highlight box is drawn; the full source sheet is shown for context.</p>\n",
            );
        }
    } else {
        body.push_str(
            "<p class=\"caption\">No resolvable <code>source_sheet</code> for this record.</p>\n",
        );
    }
    body.push_str("</section>\n");

    // --- Real game scale ---
    body.push_str(
        "<section>\n<h2>Runtime image at real game scale (1x)</h2>\n<div class=\"swatch-row\">\n",
    );
    body.push_str(&swatch(
        &href,
        display,
        sampler_cls,
        "checkerboard",
        "checkerboard",
        false,
    ));
    body.push_str(&swatch(
        &href,
        display,
        sampler_cls,
        &format!("representative-bg\" style=\"background-image:url({representative_bg_href})"),
        "representative background",
        false,
    ));
    body.push_str("</div>\n</section>\n");

    // --- Mirrored review aid ---
    body.push_str("<section>\n<h2>Mirrored (review aid)</h2>\n<div class=\"swatch-row\">\n");
    body.push_str(&swatch(
        &href,
        display,
        sampler_cls,
        "checkerboard",
        "horizontally flipped",
        true,
    ));
    body.push_str("</div>\n<p class=\"caption\">This pixel-flipped preview matches the artwork-facing change used by the live rig. \
When mirroring a whole fighter, runtime and gallery both flip each part's pixels and mirror its transform/pivot \
(including negated <code>pivot.x</code>; see <code>src/cutout.rs</code>'s <code>part_sprite</code> and \
<code>part_transform</code>). See the linked composition page(s) below for the complete mirrored placement.</p>\n</section>\n");

    // --- Pivot / attachment diagram ---
    body.push_str("<section>\n<h2>Pivot / attachment guide</h2>\n");
    body.push_str(&pivot_diagram(
        pivot,
        display,
        r.attachment.as_deref().unwrap_or("(none)"),
    ));
    body.push_str(
        "<p class=\"caption\">Coordinate convention: <code>(0, 0)</code> is the rig root; \
         <code>+x</code> is toward the character's authored-facing (right, pre-mirror) side; \
         <code>+y</code> is up. These are rig-space translation units -- the same space as \
         <code>src/cutout.rs</code>'s <code>Transform</code> translation -- not pixel \
         coordinates inside this record's own image (see \
         <code>xtask/src/assets/validate/bounds.rs</code>'s module docs).</p>\n</section>\n",
    );

    if !composition_links.is_empty() {
        body.push_str("<section>\n<h2>Compositions</h2>\n<ul>\n");
        for (label, href) in composition_links {
            body.push_str(&format!(
                "<li><a href=\"{href}\">{}</a></li>\n",
                escape_html(label)
            ));
        }
        body.push_str("</ul>\n</section>\n");
    }

    page_shell(&r.id, &body)
}

fn swatch(
    href: &str,
    display: [f32; 2],
    sampler_cls: &str,
    bg_class_and_style: &str,
    label: &str,
    mirrored: bool,
) -> String {
    let mirror_cls = if mirrored { " mirrored" } else { "" };
    format!(
        "<div>\n<div class=\"swatch {bg_class_and_style}\" style=\"width:{}px;height:{}px;\">\n\
         <img src=\"{href}\" class=\"{sampler_cls}{mirror_cls}\" style=\"width:{}px;height:{}px;\" alt=\"{label}\">\n</div>\n\
         <p class=\"swatch-label\">{label} &middot; {}&times;{} px</p>\n</div>\n",
        display[0].max(1.0),
        display[1].max(1.0),
        display[0].max(1.0),
        display[1].max(1.0),
        display[0] as i32,
        display[1] as i32,
    )
}

fn pivot_diagram(pivot: [f32; 2], display: [f32; 2], attachment: &str) -> String {
    let placement = PartPlacement { pivot, display };
    let canvas = super::layout::rig_canvas(&[placement]);
    let (ox, oy) = canvas.origin();
    let b = canvas.place(placement, false);
    format!(
        "<div class=\"rig-canvas\" style=\"width:{:.0}px;height:{:.0}px;\">\n\
         <div class=\"rig-crosshair\" style=\"left:{:.1}px;top:{:.1}px;\"></div>\n\
         <div class=\"rig-box\" style=\"left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;\" title=\"attachment: {}\"></div>\n\
         </div>\n<p class=\"swatch-label\">attachment: <code>{}</code> &middot; pivot [{:.2}, {:.2}] &middot; display [{:.2}, {:.2}]</p>\n",
        canvas.width,
        canvas.height,
        ox,
        oy,
        b.left,
        b.top,
        b.width,
        b.height,
        escape_html(attachment),
        escape_html(attachment),
        pivot[0],
        pivot[1],
        display[0],
        display[1],
    )
}

/// One placed layer for a composition page: the record to draw, its
/// z-order key, and whether it's the "gear on top of a body part" layer
/// (only used for a slightly higher z bias -- see `model::draw_order_index`
/// callers in `mod.rs`).
pub struct CompositionLayer<'a> {
    pub record: &'a ResolvedRecord,
    pub placement: PartPlacement,
    pub z: usize,
}

/// Renders a full-rig composition page: every layer drawn twice (normal and
/// mirrored facing) on a shared canvas computed from every layer's own
/// `pivot`/`display`.
pub fn render_composition_page(
    title: &str,
    id: &str,
    layers: &[CompositionLayer],
    note: &str,
) -> String {
    let placements: Vec<PartPlacement> = layers.iter().map(|l| l.placement).collect();
    let canvas = super::layout::rig_canvas(&placements);

    let mut body = format!(
        "<h1>{}</h1>\n<p class=\"caption\">{}</p>\n",
        escape_html(title),
        note
    );
    body.push_str("<div class=\"swatch-row\">\n");
    body.push_str(&facing_canvas(&canvas, layers, false, "normal facing"));
    body.push_str(&facing_canvas(&canvas, layers, true, "mirrored facing"));
    body.push_str("</div>\n");
    body.push_str("<section>\n<h2>Layers</h2>\n<ul>\n");
    let mut sorted: Vec<&CompositionLayer> = layers.iter().collect();
    sorted.sort_by_key(|l| l.z);
    for layer in sorted {
        body.push_str(&format!(
            "<li><a href=\"{}.html\">{}</a></li>\n",
            escape_html(&layer.record.record.id),
            escape_html(&layer.record.record.id)
        ));
    }
    body.push_str("</ul>\n</section>\n");
    page_shell(id, &body)
}

fn facing_canvas(
    canvas: &RigCanvas,
    layers: &[CompositionLayer],
    mirrored: bool,
    label: &str,
) -> String {
    let mut inner = String::new();
    let mut ordered: Vec<&CompositionLayer> = layers.iter().collect();
    ordered.sort_by_key(|l| l.z);
    for layer in ordered {
        let href = asset_href_resolved(layer.record);
        let sampler_cls = sampler_class(layer.record.record.sampler);
        let mirror_cls = if mirrored { " mirrored" } else { "" };
        let b: Box2D = canvas.place(layer.placement, mirrored);
        inner.push_str(&format!(
            "<img src=\"{href}\" class=\"{sampler_cls}{mirror_cls}\" style=\"left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;z-index:{};\" alt=\"{}\">\n",
            b.left, b.top, b.width, b.height, layer.z, escape_html(&layer.record.record.id),
        ));
    }
    format!(
        "<div>\n<div class=\"rig-canvas representative-bg\" style=\"width:{:.0}px;height:{:.0}px;background-image:url({});\">\n{inner}</div>\n<p class=\"swatch-label\">{label}</p>\n</div>\n",
        canvas.width,
        canvas.height,
        placeholder_bg_href(),
    )
}

fn placeholder_bg_href() -> String {
    format!("{ASSET_REL}/backgrounds/village_near.png")
}

/// Renders the 9-slice UI panel preview page.
pub fn render_ui_panel_page(record: &ResolvedRecord) -> String {
    let href = asset_href_resolved(record);
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    body.push_str(&format!(
        "<section>\n<h2>9-slice preview over a linen-toned backdrop</h2>\n\
         <p class=\"caption\">Border inset: {PANEL_BORDER_INSET_PX}px (a point-in-time snapshot of \
         <code>PANEL_BORDER_INSET</code> in <code>src/theme/mod.rs</code>). Backdrop color \
         approximates the art direction's Cream palette entry (<code>{LINEN_BACKDROP}</code>) -- \
         no literal \"linen\" texture asset exists in this repo.</p>\n\
         <div class=\"nine-slice-row\" style=\"background:{LINEN_BACKDROP};padding:24px;\">\n"
    ));
    for (w, h) in REPRESENTATIVE_PANEL_SIZES {
        body.push_str(&format!(
            "<div class=\"panel-preview nine-slice\" style=\"width:{w}px;height:{h}px;border-image-source:url({href});\">\
             <span style=\"color:#e8dcc8;font-size:12px;\">{w}&times;{h}</span></div>\n"
        ));
    }
    body.push_str("</div>\n</section>\n");
    page_shell(&record.record.id, &body)
}

/// Renders a UI icon page (icon at native size and zoomed, over checkerboard
/// and a representative panel backdrop).
pub fn render_ui_icon_page(record: &ResolvedRecord) -> String {
    let href = asset_href_resolved(record);
    let dims = record.record.dimensions.unwrap_or([32, 32]);
    let native = [dims[0] as f32, dims[1] as f32];
    let zoomed = [native[0] * 4.0, native[1] * 4.0];
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    body.push_str("<section>\n<h2>Native size and 4x zoom</h2>\n<div class=\"swatch-row\">\n");
    body.push_str(&swatch(
        &href,
        native,
        "pixelated",
        "checkerboard",
        "native size, checkerboard",
        false,
    ));
    body.push_str(&swatch(
        &href,
        zoomed,
        "pixelated",
        "checkerboard",
        "4x zoom, checkerboard",
        false,
    ));
    body.push_str(&format!(
        "<div>\n<div class=\"swatch\" style=\"width:{}px;height:{}px;background:{LINEN_BACKDROP};\">\n\
         <img src=\"{href}\" class=\"pixelated\" style=\"width:{}px;height:{}px;\" alt=\"on panel backdrop\">\n</div>\n\
         <p class=\"swatch-label\">on panel-toned backdrop</p>\n</div>\n",
        zoomed[0], zoomed[1], zoomed[0], zoomed[1]
    ));
    body.push_str("</div>\n</section>\n");
    page_shell(&record.record.id, &body)
}

/// Renders a background scene's parallax composition page (far/near/foreground layered).
pub fn render_background_scene_page(scene: &str, layers: &[&ResolvedRecord]) -> String {
    let mut body = format!("<h1>Background scene: {}</h1>\n", escape_html(scene));
    body.push_str("<p class=\"caption\">Parallax layers stacked back-to-front in id order (far, near, foreground), scaled to a shared preview frame.</p>\n");
    let dims = layers
        .first()
        .and_then(|l| l.record.dimensions)
        .unwrap_or([900, 600]);
    let preview_w = 640.0_f32;
    let preview_h = preview_w * dims[1] as f32 / dims[0] as f32;
    body.push_str(&format!(
        "<div class=\"scene-composite\" style=\"width:{preview_w}px;height:{preview_h:.0}px;\">\n"
    ));
    for layer in layers {
        body.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\">\n",
            asset_href_resolved(layer),
            escape_html(&layer.record.id)
        ));
    }
    body.push_str("</div>\n<section>\n<h2>Layers</h2>\n<ul>\n");
    for layer in layers {
        body.push_str(&format!(
            "<li><a href=\"{}.html\">{}</a></li>\n",
            escape_html(&layer.record.id),
            escape_html(&layer.record.id)
        ));
    }
    body.push_str("</ul>\n</section>\n");
    page_shell(&format!("composition.background.{scene}"), &body)
}

/// Renders a plain asset page: raster preview over checkerboard (or a
/// native `<img>` for `.svg`) plus metadata, for records with no
/// composition/rig context (source sheets, legacy overlays, sprites, web
/// assets). `related_links` optionally lists other pages this asset relates
/// to (e.g. a background layer's parallax-scene composition page).
pub fn render_generic_asset_page(
    record: &ResolvedRecord,
    related_links: &[(String, String)],
) -> String {
    let href = asset_href_resolved(record);
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    if let Some(dims) = record.record.dimensions {
        let max_w = 480.0_f32;
        let scale = (max_w / dims[0] as f32).min(3.0);
        let (w, h) = (dims[0] as f32 * scale, dims[1] as f32 * scale);
        body.push_str("<section>\n<h2>Preview</h2>\n<div class=\"swatch-row\">\n");
        body.push_str(&swatch(
            &href,
            [w, h],
            sampler_class(record.record.sampler),
            "checkerboard",
            "checkerboard",
            false,
        ));
        body.push_str("</div>\n</section>\n");
    } else {
        body.push_str(&format!(
            "<section>\n<h2>Preview</h2>\n<img src=\"{href}\" style=\"max-width:480px;\" alt=\"{}\">\n</section>\n",
            escape_html(&record.record.id)
        ));
    }
    if !related_links.is_empty() {
        body.push_str("<section>\n<h2>Related</h2>\n<ul>\n");
        for (label, href) in related_links {
            body.push_str(&format!(
                "<li><a href=\"{href}\">{}</a></li>\n",
                escape_html(label)
            ));
        }
        body.push_str("</ul>\n</section>\n");
    }
    page_shell(&record.record.id, &body)
}

/// Renders a font/font-license metadata page (no raster preview -- fonts
/// aren't images).
pub fn render_font_page(record: &ResolvedRecord, probe: &super::probe::FontProbe) -> String {
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    body.push_str("<section>\n<h2>Font metrics</h2>\n<table class=\"meta\">\n");
    body.push_str(&meta_row(
        "family",
        probe
            .family
            .clone()
            .map(|f| escape_html(&f))
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str(&meta_row(
        "units per em",
        probe
            .units_per_em
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str(&meta_row(
        "glyph count",
        probe
            .glyph_count
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str("</table>\n</section>\n");
    page_shell(&record.record.id, &body)
}

/// Renders a document (font-license) metadata page: no raster preview, a
/// relative link to open the license text directly.
pub fn render_document_page(record: &ResolvedRecord) -> String {
    let href = asset_href_resolved(record);
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    body.push_str(&format!("<p><a href=\"{href}\">Open document</a></p>\n"));
    page_shell(&record.record.id, &body)
}

/// Renders an audio metadata page (no fake raster preview -- a native
/// `<audio>` element for convenience, plus best-effort probed metrics).
pub fn render_audio_page(record: &ResolvedRecord, probe: &super::probe::AudioProbe) -> String {
    let href = asset_href_resolved(record);
    let mut body = format!(
        "<h1>{}</h1>\n{}\n",
        escape_html(&record.record.id),
        metadata_table(record)
    );
    body.push_str("<section>\n<h2>Audio metrics (best effort)</h2>\n<table class=\"meta\">\n");
    body.push_str(&meta_row(
        "duration",
        probe
            .duration_seconds
            .map(|d| format!("{d:.2}s"))
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str(&meta_row(
        "sample rate",
        probe
            .sample_rate
            .map(|r| format!("{r} Hz"))
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str(&meta_row(
        "channels",
        probe
            .channels
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    body.push_str("</table>\n<audio controls src=\"");
    body.push_str(&href);
    body.push_str("\"></audio>\n</section>\n");
    page_shell(&record.record.id, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Category, Kind, Record, Status};
    use std::path::PathBuf;

    fn part_record(id: &str) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/fighters/human/runtime/manifest.toml"),
            full_path: PathBuf::from("fighters/human/runtime/head.png"),
            record: Record {
                id: id.to_string(),
                path: "head.png".to_string(),
                kind: Kind::Image,
                category: Category::FighterRuntimePart,
                status: Status::Runtime,
                provenance: "cropped-from-source-sheet".to_string(),
                license: "Same as project assets unless superseded".to_string(),
                generator: None,
                source_sheet: None,
                license_file: None,
                dimensions: Some([70, 92]),
                sampler: Some(Sampler::Linear),
                attachment: Some("head".to_string()),
                pivot: Some([4.0, 60.0]),
                display: Some([38.0, 42.0]),
                crop: Some("unknown".to_string()),
            },
        }
    }

    #[test]
    fn escape_html_escapes_the_five_special_characters() {
        assert_eq!(escape_html("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }

    #[test]
    fn part_page_documents_the_coordinate_convention_and_shows_both_facings() {
        let record = part_record("fighters.human.runtime.head");
        let html = render_part_page(
            &record,
            None,
            "../../../assets/backgrounds/village_near.png",
            &[],
        );
        assert!(html.contains("rig-space translation units"));
        assert!(html.contains("Mirrored (review aid)"));
        assert!(html.contains("mirrored"));
        assert!(html.contains("pivot [4.00, 60.00]"));
        assert!(html.contains("<!doctype html>"));
    }

    #[test]
    fn composition_page_places_both_facings_and_lists_layers() {
        let record = part_record("fighters.human.runtime.head");
        let layer = CompositionLayer {
            record: &record,
            placement: PartPlacement {
                pivot: [0.0, 0.0],
                display: [38.0, 42.0],
            },
            z: 0,
        };
        let html = render_composition_page(
            "Human (neutral pose)",
            "composition.human",
            &[layer],
            "test note",
        );
        assert!(html.contains("normal facing"));
        assert!(html.contains("mirrored facing"));
        assert!(html.contains("scaleX(-1)"));
        assert!(html.contains("class=\"smooth mirrored\""));
        assert!(html.contains("fighters.human.runtime.head"));
    }
}
