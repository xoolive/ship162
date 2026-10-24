use std::{
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn run_with_sources(captures: &[Option<&[u8]>]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let mut config = String::from("verbose = true\n");
    for (index, bytes) in captures.iter().enumerate() {
        let path = directory.path().join(format!("capture-{index}.cu8"));
        if let Some(bytes) = bytes {
            std::fs::write(&path, bytes).unwrap();
        }
        config.push_str(&format!("\n[[sources]]\niqfile = {path:?}\n"));
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
