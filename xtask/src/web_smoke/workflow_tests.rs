//! Structural policy checks, intentionally limited to this workflow's job layout.
//! GitHub remains the executor/validator of expressions; these tests pin the
//! failure guard and opt-in expressions, not an invented Actions interpreter.
use std::collections::BTreeMap;

use super::{SCENARIOS, build::Variant, scenario_variant};

const WORKFLOW: &str = include_str!("../../../.github/workflows/web-smoke.yml");

fn jobs() -> BTreeMap<&'static str, &'static str> {
    let text = WORKFLOW.split_once("\njobs:\n").unwrap().1;
    let mut offset = 0;
    let mut starts = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("  ") && !line.starts_with("   ") && line.trim().ends_with(':') {
            starts.push((line.trim().trim_end_matches(':'), offset));
        }
        offset += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, (name, start))| {
            (
                *name,
                &text[*start..starts.get(i + 1).map_or(text.len(), |(_, next)| *next)],
            )
        })
        .collect()
}

#[test]
fn two_same_run_producers_feed_the_existing_named_fanout_without_dependency_skips() {
    let jobs = jobs();
    let expected = [
        ("cold-menu", "cold-menu (desktop + phone)"),
        (
            "gold-journey",
            "gold-journey (1440x900 DPR-1 normal desktop; full matrix opt-in)",
        ),
        (
            "accessibility-settings-reload",
            "accessibility-settings-reload (a11y toggles persist + zoom permitted)",
        ),
        (
            "reduced-motion-fight",
            "reduced-motion-fight (loader CSS + parallax/shake/lunge suppression)",
        ),
        (
            "fight-palette-desktop",
            "fight-palette-desktop (descriptor-driven action bar fits the stage)",
        ),
        (
            "high-contrast",
            "high-contrast (persisted preference switches the palette, desktop + phone)",
        ),
        (
            "fight-palette-phone",
            "fight-palette-phone (category disclosure fits a 390x844 stage) [manual/full_matrix only]",
        ),
        (
            "fight-palette-accessible",
            "fight-palette-accessible (real-keyboard focus, disabled reasons, category-close recovery)",
        ),
        (
            "keyboard-accessibility",
            "keyboard-accessibility (keyboard-only nav + audio controls, every screen)",
        ),
        (
            "zoom-200",
            "zoom-200 (required screens playable at 200% desktop zoom) [manual/full_matrix only]",
        ),
        (
            "corrupt-save-recovery",
            "corrupt-save-recovery (Romanian recovery UI, run-snapshot-only quarantine)",
        ),
        (
            "touch-targets",
            "touch-targets (interactive targets meet 44x44 CSS px) [manual/full_matrix only]",
        ),
        (
            "save-reload",
            "save-reload (result -> shop -> real reload resumes at Shop, run values intact)",
        ),
        (
            "abandon-forfeit",
            "abandon-forfeit (Abandonează clears the run snapshot, disables Continuă)",
        ),
    ];
    assert_eq!(jobs.len(), expected.len() + 2);
    assert_eq!(WORKFLOW.matches(" --build ").count(), 2);
    for variant in [Variant::Production, Variant::Review] {
        let kind = variant.name();
        let producer = jobs[format!("build-{kind}").as_str()];
        assert!(producer.contains(&format!(
            "web-smoke --build {kind} --artifact-root target/web-smoke-bundles"
        )));
        assert!(producer.contains("uses: actions/upload-artifact@v7"));
        assert!(producer.contains("if-no-files-found: error"));
        assert!(!producer.contains("if: always()"));
        assert!(!producer.contains("--scenario"));
        assert!(producer.contains(&format!(
            "name: web-smoke-bundle-{kind}-${{{{ github.run_id }}}}-${{{{ github.run_attempt }}}}"
        )));
    }
    for (scenario, name) in expected {
        let job = jobs[scenario];
        let kind = scenario_variant(scenario).unwrap().name();
        assert!(
            job.contains(&format!("    name: {name}\n")),
            "changed check name: {scenario}"
        );
        assert!(job.contains(&format!("    needs: build-{kind}\n")));
        let opt_in = matches!(
            scenario,
            "fight-palette-phone" | "touch-targets" | "zoom-200"
        );
        let condition = if opt_in {
            "${{ !cancelled() && github.event_name == 'workflow_dispatch' && inputs.full_matrix }}"
        } else {
            "${{ !cancelled() }}"
        };
        assert!(job.contains(&format!("    if: {condition}\n")));
        assert!(job.contains(&format!(
            "if: needs.build-{kind}.result != 'success'\n      run: exit 1"
        )));
        assert!(job.contains("uses: actions/download-artifact@v8"));
        assert!(job.contains(&format!(
            "name: web-smoke-bundle-{kind}-${{{{ github.run_id }}}}-${{{{ github.run_attempt }}}}"
        )));
        assert!(job.contains(&format!("path: target/web-smoke-bundles/{kind}/")));
        assert!(job.contains(&format!(
            "web-smoke --scenario {scenario} --artifact-root target/web-smoke-bundles"
        )));
        assert!(!job.contains(" --build "));
        for selector in [
            "run-id:",
            "repository:",
            "github-token:",
            "continue-on-error:",
        ] {
            assert!(
                !job.lines()
                    .any(|line| line.trim_start().starts_with(selector)),
                "active {selector} in {scenario}"
            );
        }
    }
    assert!(!WORKFLOW.contains("pull_request_target"));
}

#[test]
fn production_review_and_opt_in_policies_are_explicit() {
    for scenario in SCENARIOS {
        assert_eq!(
            scenario_variant(scenario).unwrap(),
            if matches!(*scenario, "cold-menu" | "accessibility-settings-reload") {
                Variant::Production
            } else {
                Variant::Review
            }
        );
    }
    assert!(scenario_variant("unknown").is_err());
    let jobs = jobs();
    for scenario in ["cold-menu", "gold-journey"] {
        assert!(jobs[scenario].contains("${{ github.event_name == 'workflow_dispatch' && inputs.strict_visual && '--strict-visual' || '' }}"));
    }
    assert!(jobs["gold-journey"].contains(
        "${{ github.event_name == 'workflow_dispatch' && inputs.full_matrix && '1' || '' }}"
    ));
    assert_eq!(WORKFLOW.matches("&& inputs.strict_visual").count(), 2);
}
