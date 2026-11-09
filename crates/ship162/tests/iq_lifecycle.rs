use std::{
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn run_with_sources(captures: &[Option<&[u8]>]) -> Output {
    let names: Vec<_> = (0..captures.len())
        .map(|index| format!("capture-{index}.cu8"))
        .collect();
    let captures: Vec<_> = captures
        .iter()
        .zip(&names)
        .map(|(bytes, name)| (name.as_str(), *bytes, ""))
        .collect();
    run_named_sources(&captures)
}

fn run_named_sources(captures: &[(&str, Option<&[u8]>, &str)]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let mut config = String::from("verbose = true\n");
    for (name, bytes, settings) in captures {
        let path = directory.path().join(name);
        if let Some(bytes) = bytes {
            std::fs::write(&path, bytes).unwrap();
        }
        config.push_str(&format!("\n[[sources]]\niqfile = {path:?}\n{settings}\n"));
    }
    let config_path = directory.path().join("config.toml");
    std::fs::write(&config_path, config).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ship162"))
        .env("XDG_CONFIG_HOME", directory.path())
        .env("SHIP162_CONFIG", config_path)
        .env("RUST_LOG", "warn")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "finite IQ sources did not exit: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn filename_inferred_rate_decodes_the_96khz_ais_fixture() {
    let capture = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rs162/data/ais_96k.bin"),
    )
    .unwrap();
    let output = run_named_sources(&[("rtlsdr_162000000_96000_cu8.iq", Some(&capture), "")]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages.len(), 56);
}

#[test]
fn inferred_and_overridden_formats_reach_the_replay_source() {
    // One CU8 sample, but not enough bytes for a CF32 sample. This distinguishes
    // filename inference from the legacy hardcoded CU8 replay path.
    let filename = "gqrx_20260603_083011_162000000_96000_fc.raw";
    let inferred = run_named_sources(&[(filename, Some(&[128, 128]), "")]);
    assert!(!inferred.status.success());
    assert!(String::from_utf8_lossy(&inferred.stderr)
        .to_lowercase()
        .contains("truncat"));
    let overridden = run_named_sources(&[(
        filename,
        Some(&[128, 128]),
        "format = 'U8'\nsample_rate = '288 kHz'",
    )]);
    assert!(
        overridden.status.success(),
        "{}",
        String::from_utf8_lossy(&overridden.stderr)
    );
}

#[test]
fn empty_capture_exits_successfully() {
    let output = run_with_sources(&[Some(&[])]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn short_complete_captures_all_finish() {
    // Less than the requested chunk size, but a whole number of CU8 IQ samples.
    let output = run_with_sources(&[Some(&[128; 2048]), Some(&[128; 64])]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn truncated_capture_returns_failure() {
    let output = run_with_sources(&[Some(&[128])]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.to_lowercase().contains("truncat"), "{stderr}");
}

#[test]
fn missing_capture_returns_failure_even_with_a_successful_source() {
    let output = run_with_sources(&[Some(&[]), None]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("capture-1.cu8"), "{stderr}");
}
