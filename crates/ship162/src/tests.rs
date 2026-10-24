use super::*;

#[test]
fn upper_lower_and_arrow_keys_scroll_the_active_view() {
    let mut state = AppState::new();
    for key in [KeyCode::Char('k'), KeyCode::Char('K'), KeyCode::Up] {
        state.scroll_offset = 2;
        navigate(&mut state, key, 20);
        assert_eq!(state.scroll_offset, 1);
    }
    let subscriber = tracing_subscriber::registry()
        .with(diagnostics::UiEventLayer::new(state.diagnostics.clone()));
    tracing::subscriber::with_default(subscriber, || {
        for index in 0..5 {
            tracing::error!(source = "test", index, "distinct diagnostic");
        }
    });
    state.show_errors = true;
    for key in [KeyCode::Char('j'), KeyCode::Char('J'), KeyCode::Down] {
        state.error_scroll = 0;
        navigate(&mut state, key, 20);
        assert_eq!(state.error_scroll, 1);
        assert_eq!(state.scroll_offset, 1);
    }
    for key in [KeyCode::Char('k'), KeyCode::Char('K'), KeyCode::Up] {
        state.error_scroll = 2;
        navigate(&mut state, key, 20);
        assert_eq!(state.error_scroll, 1);
    }
}

#[test]
fn home_and_g_return_only_the_active_view_to_top() {
    let mut state = AppState::new();
    for key in [
        KeyCode::Char('g'),
        KeyCode::Char('G'),
        KeyCode::Home,
        KeyCode::PageUp,
    ] {
        state.show_errors = false;
        state.scroll_offset = 10;
        state.error_scroll = 3;
        navigate(&mut state, key, 20);
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.error_scroll, 3);
        state.show_errors = true;
        state.scroll_offset = 5;
        navigate(&mut state, key, 20);
        assert_eq!(state.error_scroll, 0);
        assert_eq!(state.scroll_offset, 5);
    }
}

#[cfg(feature = "mqtt")]
#[test]
fn all_three_network_sources_are_counted_before_any_connection_event() {
    use crate::status::{SourceReporter, SourceStatus};
    let options: Options = toml::from_str(
        r#"
        [[sources]]
        tcp = "127.0.0.1:5631"
        [[sources]]
        mqtt = "mqtt://mqtt.digitraffic.fi"
        [[sources]]
        ws = "ws://127.0.0.1:9876/45678"
    "#,
    )
    .unwrap();
    let mut state = initial_source_state(&options.sources);
    assert_eq!(state.source_statuses().count(), 3);
    assert!(state
        .source_statuses()
        .all(|event| matches!(event.status(), SourceStatus::Connecting)));
    assert!(state
        .source_statuses()
        .any(|event| event.to_string().contains("mqtt")));
    // Updating MQTT replaces its registered entry without changing the total.
    let (tx, mut rx) = mpsc::unbounded_channel();
    SourceReporter::new(SourceId::from_index(1), "mqtt", tx).report(SourceStatus::Connected);
    state.set_source_status(rx.try_recv().unwrap());
    assert_eq!(state.source_statuses().count(), 3);
    assert_eq!(
        state
            .source_statuses()
            .filter(|event| matches!(event.status(), SourceStatus::Connected))
            .count(),
        1
    );
}

#[cfg(feature = "rtlsdr")]
#[test]
fn invalid_sample_rates_are_rejected_before_source_startup() {
    for value in [
        "0",
        "-1",
        "nan",
        "inf",
        "288000.5",
        "12288001",
        "4294967296",
    ] {
        let source =
            toml::from_str(&format!("rtlsdr = {{ device = 0 }}\nsample_rate = {value}")).unwrap();
        assert!(validate_sources(&[source]).is_err(), "accepted {value}");
    }
    let source = toml::from_str("rtlsdr = { device = 0 }\nsample_rate = 288000").unwrap();
    validate_sources(&[source]).unwrap();
}
