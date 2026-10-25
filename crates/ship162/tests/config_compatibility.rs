use std::process::Command;

fn help_with_config(config: &str) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, config).unwrap();
    // --help parses the config but exits before Redis, MQTT, TCP or USB startup.
    Command::new(env!("CARGO_BIN_EXE_ship162"))
        .arg("--help")
        .env("SHIP162_CONFIG", path)
        .env("XDG_CONFIG_HOME", directory.path())
        .output()
        .unwrap()
}

#[test]
fn tcp_and_websocket_config_loads_in_every_build() {
    let output = help_with_config(
        r#"
        [[sources]]
        tcp = "127.0.0.1:5631"
        [[sources]]
        ws = "ws://127.0.0.1:9876/45678"
    "#,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(all(feature = "mqtt", feature = "rtlsdr"))]
#[test]
fn mixed_network_and_nested_rtl_config_loads_without_starting_sources() {
    let output = help_with_config(
        r#"
        redis_url = "redis://localhost:6379"
        interactive = true
        prevent_sleep = true
        [[sources]]
        tcp = "127.0.0.1:5631"
        [[sources]]
        mqtt = "mqtt://mqtt.digitraffic.fi"
        [[sources]]
        ws = "ws://127.0.0.1:9876/45678"
        [[sources]]
        rtlsdr = { device = 0, sample_rate = 2304000 }
    "#,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: ship162"));
}

#[cfg(feature = "soapy")]
#[test]
fn structured_soapy_args_and_rate_load_with_soapy_enabled() {
    let output = help_with_config(
        r#"
        [[sources]]
        soapy = { args = "driver=airspy", sample_rate = 3000000 }
        gain = { LNA = 13, MIX = 3, VGA = 13 }
    "#,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(not(feature = "mqtt"))]
#[test]
fn mixed_config_with_disabled_mqtt_explains_missing_feature() {
    let output = help_with_config(
        r#"
        [[sources]]
        tcp = "127.0.0.1:5631"
        [[sources]]
        mqtt = "mqtt://mqtt.digitraffic.fi"
        [[sources]]
        ws = "ws://127.0.0.1:9876/45678"
    "#,
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("MQTT support is not enabled"), "{error}");
    assert!(error.contains("--features mqtt"), "{error}");
    assert!(!error.contains("untagged enum"), "{error}");
}
