use key_reset_follow::cycle::{Decision::*, Sample, WEEK, classify};
fn s(t: i64, r: i64, used: f64) -> Sample {
    Sample {
        observed: t,
        reset: r,
        used: Some(used),
    }
}
#[test]
fn independent_cycle_evidence() {
    let old = s(1_000_000, WEEK, 90.0);
    let cases = [
        (s(2_000_000, WEEK, 0.0), Same),
        (s(WEEK + 10_000, WEEK, 90.0), Same),
        (s(2_000_000, WEEK + 120_000, 0.0), Same),
        (s(2_000_000, WEEK - 120_001, 0.0), Unexplained),
        (s(1_000_000, WEEK * 2, 0.0), Stale),
        (s(WEEK + 10_000, WEEK * 2, 0.0), Normal),
        (s(WEEK * 4, WEEK * 5, 0.0), Normal),
        (s(2_000_000, WEEK + 1_500_000, 0.0), Early),
        (s(2_000_000, WEEK + 5_000_000, 0.0), Unexplained),
        (s(2_000_000, WEEK + 500_000, 0.0), Unexplained),
    ];
    for (sample, expected) in cases {
        assert_eq!(classify(&old, &sample), expected, "{sample:?}");
    }
}
#[test]
fn exact_window_identity_is_required() {
    use gateway_plugin_sdk::call::data::{QuotaFacts, QuotaWindowFacts};
    let mut facts = QuotaFacts {
        schema_version: 1,
        account_id: "test".into(),
        observed_at_ms: Some(1),
        windows: vec![QuotaWindowFacts {
            key: "codex:604800s".into(),
            window_seconds: Some(604800),
            used_percent: None,
            reset_at_ms: Some(WEEK),
        }],
    };
    assert!(key_reset_follow::cycle::sample(&facts).is_ok());
    facts.windows.push(facts.windows[0].clone());
    assert!(key_reset_follow::cycle::sample(&facts).is_err());
    facts.windows.pop();
    facts.windows[0].key.push_str(":secondary_window");
    assert!(key_reset_follow::cycle::sample(&facts).is_err());
    facts.windows.clear();
    assert!(key_reset_follow::cycle::sample(&facts).is_err());
}
#[test]
fn manifest_matches_target_contract() {
    let manifest_bytes = include_bytes!("../plugin.json");
    let manifest = gateway_plugin_sdk::Manifest::from_author_slice(manifest_bytes).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(manifest_bytes).unwrap();
    assert_eq!(manifest.plugin_id().unwrap(), "tumi.key-reset-follow");
    assert_eq!(manifest.version.to_string(), env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest.author.as_deref(), Some("tumi"));
    assert_eq!(gateway_plugin_sdk::PROTOCOL_VERSION, 7);
    assert_eq!(raw["engines"]["codex-proxy-rs"], ">=3.33.0");
    assert_eq!(raw["contributes"]["management"]["version"], 2);
    assert_eq!(raw["contributes"]["maintenance"]["version"], 1);
    for (host, allowed) in [
        ("3.18.3", false),
        ("3.29.0", false),
        ("3.29.1", false),
        ("3.29.6", false),
        ("3.29.7", false),
        ("3.29.8", false),
        ("3.30.0-beta.1", false),
        ("3.30.0", false),
        ("3.31.1", false),
        ("3.31.2-beta.1", false),
        ("3.32.0", false),
        ("3.33.0-beta.1", false),
        ("3.33.0", true),
        ("3.33.1", true),
        ("4.0.0", true),
        ("99.0.0", true),
    ] {
        assert_eq!(
            manifest
                .engines
                .codex_proxy_rs
                .matches(&host.parse().unwrap()),
            allowed,
            "{host}"
        );
    }
}
