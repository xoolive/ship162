//! Declarative configuration input, separate from validated runtime sources.

use std::collections::BTreeMap;

use desperado::Gain;
use serde::{de::IgnoredAny, Deserialize};

use super::{AddressPath, IqFileConfig, Source, TcpConfig, WebSocketConfig, WsPath};
use crate::status::RetryPolicy;

#[cfg(feature = "mqtt")]
type RawMqtt = String;
#[cfg(not(feature = "mqtt"))]
type RawMqtt = IgnoredAny;
#[cfg(feature = "airspy")]
type RawAirspy = desperado::sdr::AirspyPath;
#[cfg(not(feature = "airspy"))]
type RawAirspy = IgnoredAny;
#[cfg(feature = "hackrf")]
type RawHackrf = desperado::sdr::HackrfPath;
#[cfg(not(feature = "hackrf"))]
type RawHackrf = IgnoredAny;
#[cfg(not(feature = "rtlsdr"))]
type RawRtlSdr = IgnoredAny;
#[cfg(not(feature = "soapy"))]
type RawSoapy = IgnoredAny;

const ONE_BACKEND: &str =
    "a source must specify exactly one of tcp, ws, mqtt, rtlsdr, soapy, airspy, hackrf or iqfile";

/// Known optional backends remain recognizable in builds without their feature.
#[derive(Deserialize)]
pub(super) struct RawSource {
    tcp: Option<AddressPath>,
    ws: Option<WsPath>,
    mqtt: Option<RawMqtt>,
    rtlsdr: Option<RawRtlSdr>,
    soapy: Option<RawSoapy>,
    airspy: Option<RawAirspy>,
    hackrf: Option<RawHackrf>,
    iqfile: Option<String>,
    retry: Option<RetryPolicy>,
    #[serde(flatten)]
    settings: RawSdrSettings,
    // Do not combine deny_unknown_fields with flatten. Keep all leftovers and
    // reject them explicitly, with the selected backend's name, in TryFrom.
    #[serde(flatten)]
    extra: BTreeMap<String, IgnoredAny>,
}

#[derive(Deserialize)]
struct RawSdrSettings {
    gain: Option<Gain>,
    sample_rate: Option<f64>,
    bias_tee: Option<bool>,
}

// The upstream selector denies unknown fields, so it cannot be flattened into
// a larger table containing sample_rate. This compatibility input is strict too.
#[cfg(feature = "rtlsdr")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRtlSdr {
    device: Option<usize>,
    serial: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
    sample_rate: Option<f64>,
}

#[cfg(feature = "soapy")]
#[derive(Deserialize)]
#[serde(
    untagged,
    expecting = "a SoapySDR argument string or a table with string `args` and numeric `sample_rate`"
)]
enum RawSoapy {
    Short(String),
    Long(RawSoapyTable),
}

#[cfg(feature = "soapy")]
#[derive(Deserialize)]
struct RawSoapyTable {
    args: String,
    sample_rate: Option<f64>,
    #[serde(flatten)]
    extra: BTreeMap<String, IgnoredAny>,
}

fn reject_unknown_fields(
    kind: &str,
    scope: &str,
    fields: &BTreeMap<String, IgnoredAny>,
) -> Result<(), String> {
    if let Some(field) = fields.keys().next() {
        return Err(format!("invalid `{kind}` {scope}: unknown field `{field}`"));
    }
    Ok(())
}

#[cfg(any(feature = "rtlsdr", feature = "soapy"))]
fn merge_sample_rate(
    source: Option<f64>,
    nested: Option<f64>,
    kind: &str,
) -> Result<Option<f64>, String> {
    if let (Some(source), Some(nested)) = (source, nested) {
        if source != nested {
            return Err(format!("conflicting `sample_rate` and `{kind}.sample_rate`; specify one value or matching values"));
        }
    }
    Ok(source.or(nested))
}

impl TryFrom<RawSource> for Source {
    type Error = String;

    fn try_from(input: RawSource) -> Result<Self, Self::Error> {
        let mut kinds = [
            ("tcp", input.tcp.is_some()),
            ("ws", input.ws.is_some()),
            ("mqtt", input.mqtt.is_some()),
            ("rtlsdr", input.rtlsdr.is_some()),
            ("soapy", input.soapy.is_some()),
            ("airspy", input.airspy.is_some()),
            ("hackrf", input.hackrf.is_some()),
            ("iqfile", input.iqfile.is_some()),
        ]
        .into_iter()
        .filter_map(|(kind, present)| present.then_some(kind));
        let kind = kinds.next().ok_or(ONE_BACKEND)?;
        if kinds.next().is_some() {
            return Err(ONE_BACKEND.into());
        }
        reject_unknown_fields(kind, "source", &input.extra)?;
        if !matches!(kind, "rtlsdr" | "soapy" | "airspy" | "hackrf") {
            for (field, present) in [
                ("gain", input.settings.gain.is_some()),
                ("sample_rate", input.settings.sample_rate.is_some()),
                ("bias_tee", input.settings.bias_tee.is_some()),
            ] {
                if present {
                    return Err(format!("invalid `{kind}` source: unknown field `{field}`"));
                }
            }
        }
        if input.retry.is_some() && !matches!(kind, "tcp" | "ws") {
            return Err(format!("invalid `{kind}` source: unknown field `retry`"));
        }
        match input {
            RawSource {
                tcp: Some(tcp),
                retry,
                ..
            } => Ok(Self::Tcp(TcpConfig {
                tcp,
                retry: retry.unwrap_or_default(),
            })),
            RawSource {
                ws: Some(ws),
                retry,
                ..
            } => Ok(Self::WebSocket(WebSocketConfig {
                ws,
                retry: retry.unwrap_or_default(),
            })),
            RawSource {
                iqfile: Some(iqfile),
                ..
            } => Ok(Self::IqFile(IqFileConfig { iqfile })),
            #[cfg(feature = "mqtt")]
            RawSource {
                mqtt: Some(mqtt), ..
            } => Ok(Self::Mqtt(super::MqttConfig { mqtt })),
            #[cfg(not(feature = "mqtt"))]
            RawSource { mqtt: Some(_), .. } => Err(super::SourceParseError::FeatureDisabled {
                source_name: "MQTT",
                feature: "mqtt",
            }
            .to_string()),
            #[cfg(feature = "rtlsdr")]
            RawSource {
                rtlsdr: Some(device),
                settings,
                ..
            } => Ok(Self::RtlSdr(super::RtlSdrConfig {
                rtlsdr: desperado::sdr::RtlSdrPath {
                    config: desperado::sdr::RtlSdrDeviceConfig {
                        device: device.device,
                        serial: device.serial,
                        manufacturer: device.manufacturer,
                        product: device.product,
                    },
                },
                gain: settings.gain,
                sample_rate: merge_sample_rate(settings.sample_rate, device.sample_rate, kind)?,
                bias_tee: settings.bias_tee,
            })),
            #[cfg(not(feature = "rtlsdr"))]
            RawSource {
                rtlsdr: Some(_), ..
            } => Err(super::SourceParseError::FeatureDisabled {
                source_name: "RTL-SDR",
                feature: "rtlsdr",
            }
            .to_string()),
            #[cfg(feature = "soapy")]
            RawSource {
                soapy: Some(device),
                settings,
                ..
            } => {
                let (args, nested_rate) = match device {
                    RawSoapy::Short(args) => (args, None),
                    RawSoapy::Long(table) => {
                        reject_unknown_fields(kind, "device settings", &table.extra)?;
                        (table.args, table.sample_rate)
                    }
                };
                Ok(Self::Soapy(super::SoapyConfig {
                    soapy: desperado::sdr::SoapyPath { soapy: args },
                    gain: settings.gain,
                    sample_rate: merge_sample_rate(settings.sample_rate, nested_rate, kind)?,
                    bias_tee: settings.bias_tee,
                }))
            }
            #[cfg(not(feature = "soapy"))]
            RawSource { soapy: Some(_), .. } => Err(super::SourceParseError::FeatureDisabled {
                source_name: "SoapySDR",
                feature: "soapy",
            }
            .to_string()),
            #[cfg(feature = "airspy")]
            RawSource {
                airspy: Some(airspy),
                settings,
                ..
            } => {
                desperado::sdr::validate_airspy_gain_conflict(&settings.gain, &airspy.config)?;
                Ok(Self::Airspy(super::AirspyConfig {
                    airspy,
                    gain: settings.gain,
                    sample_rate: settings.sample_rate,
                    bias_tee: settings.bias_tee,
                }))
            }
            #[cfg(not(feature = "airspy"))]
            RawSource {
                airspy: Some(_), ..
            } => Err(super::SourceParseError::FeatureDisabled {
                source_name: "Airspy",
                feature: "airspy",
            }
            .to_string()),
            #[cfg(feature = "hackrf")]
            RawSource {
                hackrf: Some(hackrf),
                settings,
                ..
            } => {
                desperado::sdr::validate_hackrf_gain_conflict(&settings.gain, &hackrf.config)?;
                Ok(Self::HackRf(super::HackRfConfig {
                    hackrf,
                    gain: settings.gain,
                    sample_rate: settings.sample_rate,
                    bias_tee: settings.bias_tee,
                }))
            }
            #[cfg(not(feature = "hackrf"))]
            RawSource {
                hackrf: Some(_), ..
            } => Err(super::SourceParseError::FeatureDisabled {
                source_name: "HackRF",
                feature: "hackrf",
            }
            .to_string()),
            _ => Err(ONE_BACKEND.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Source;

    #[test]
    fn raw_source_requires_exactly_one_backend() {
        for input in [
            "",
            "gain = 20",
            "tcp = 'localhost:5631'\nws = 'ws://localhost:8080'",
            "iqfile = 'capture.cu8'\nmqtt = 'client-id'",
        ] {
            let error = toml::from_str::<Source>(input).unwrap_err().to_string();
            assert!(error.contains("exactly one"), "{error}");
        }
    }

    #[test]
    fn flattened_settings_are_rejected_for_non_sdr_sources() {
        for (field, input) in [
            ("gain", "tcp = 'localhost:5631'\ngain = 20"),
            (
                "sample_rate",
                "ws = 'ws://localhost:8080'\nsample_rate = 288000",
            ),
            ("bias_tee", "iqfile = 'capture.cu8'\nbias_tee = false"),
            (
                "retry",
                "iqfile = 'capture.cu8'\nretry = { strategy = 'fixed', delay_seconds = 1 }",
            ),
        ] {
            let error = toml::from_str::<Source>(input).unwrap_err().to_string();
            assert!(
                error.contains(&format!("unknown field `{field}`")),
                "{error}"
            );
        }
    }

    #[test]
    fn flatten_does_not_discard_unknown_scalar_or_table_fields() {
        for input in [
            "tcp = 'localhost:5631'\nsample_rte = 288000",
            "tcp = 'localhost:5631'\nsample_rte = { nested = { anything = [1, 2] } }",
        ] {
            let error = toml::from_str::<Source>(input).unwrap_err().to_string();
            assert!(error.contains("invalid `tcp` source"), "{error}");
            assert!(error.contains("unknown field `sample_rte`"), "{error}");
        }
    }

    #[cfg(not(all(
        feature = "mqtt",
        feature = "rtlsdr",
        feature = "soapy",
        feature = "airspy",
        feature = "hackrf"
    )))]
    #[test]
    fn disabled_backends_report_the_feature_even_for_unrecognized_payloads() {
        for (name, input, enabled) in [
            (
                "mqtt",
                "mqtt = { unexpected = true }",
                cfg!(feature = "mqtt"),
            ),
            ("rtlsdr", "rtlsdr = 'unexpected'", cfg!(feature = "rtlsdr")),
            ("soapy", "soapy = 42", cfg!(feature = "soapy")),
            ("airspy", "airspy = [42]", cfg!(feature = "airspy")),
            ("hackrf", "hackrf = false", cfg!(feature = "hackrf")),
        ] {
            if !enabled {
                let error = toml::from_str::<Source>(input).unwrap_err().to_string();
                assert!(error.contains("support is not enabled"), "{error}");
                assert!(error.contains(&format!("--features {name}")), "{error}");
            }
        }
    }

    #[cfg(any(feature = "airspy", feature = "hackrf"))]
    #[test]
    fn raw_source_preserves_per_element_gain_conflicts() {
        for (input, enabled) in [
            (
                "airspy = { lna_gain = 3 }\ngain = 20",
                cfg!(feature = "airspy"),
            ),
            (
                "hackrf = { lna_gain = 20 }\ngain = 20",
                cfg!(feature = "hackrf"),
            ),
        ] {
            if enabled {
                let error = toml::from_str::<Source>(input).unwrap_err().to_string();
                assert!(error.contains("Cannot specify both"), "{error}");
                assert!(error.contains("per-element"), "{error}");
            }
        }
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn invalid_soapy_tables_describe_the_expected_fields() {
        for input in [
            "soapy = { args = 42 }",
            "soapy = { args = 'driver=airspy', sample_rate = 'fast' }",
        ] {
            let error = toml::from_str::<Source>(input).unwrap_err().to_string();
            assert!(error.contains("SoapySDR"), "{error}");
            assert!(error.contains("args"), "{error}");
            assert!(error.contains("sample_rate"), "{error}");
            assert!(!error.contains("untagged enum"), "{error}");
        }
    }
}
