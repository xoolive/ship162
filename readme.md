<div align="center">
  <img src="docs/images/favicon.png" alt="ship162 Logo" width="200"/>
</div>

# ship162

**ship162** is a lightweight maritime AIS receiver and decoder, the maritime equivalent of [jet1090](https://github.com/xoolive/jet1090/) for aviation.

It decodes AIS (Automatic Identification System) messages from SDR hardware, TCP feeds, WebSocket streams, and MQTT brokers, and can display them in a real-time terminal dashboard.

![ship162 TUI displaying real-time AIS messages](./docs/images/ship162.png)

## Installation

Pre-built binaries for Linux, macOS, and Windows are available on the [GitHub Releases](https://github.com/xoolive/ship162/releases) page.

**Shell installer** (Linux and macOS):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/xoolive/ship162/releases/latest/download/ship162-installer.sh | sh
```

**Homebrew** (macOS):

```sh
brew install xoolive/homebrew/ship162
```

**Cargo**:

```sh
cargo install ship162
```

**Arch OS** (AUR):

```sh
yay -S ship162-bin
```

### Building from source

The default build includes RTL-SDR, Airspy, HackRF, and SSH support:

```sh
cargo install --git https://github.com/xoolive/ship162
```

Prebuilt binaries include MQTT support. When building from source it is opt-in (it uses the pure-Rust [rumqttc](https://github.com/bytebeamio/rumqtt) client, no C toolchain needed):

```sh
cargo install ship162 --features mqtt
```

### Linux: detach the kernel DVB driver

The RTL-SDR, Airspy, and HackRF backends use a pure-Rust USB driver ([nusb](https://github.com/kevinmehall/nusb)) that talks directly to the USB subsystem. On Linux you need to unload the kernel DVB modules first so they do not hold the device:

```sh
sudo modprobe -r dvb_usb_rtl28xxu rtl2832
```

To make this permanent, blacklist the module:

```sh
echo 'blacklist dvb_usb_rtl28xxu' | sudo tee /etc/modprobe.d/rtlsdr.conf
```

## Usage

Run `ship162 --help` for the full list of options. The most common invocations:

```sh
# Interactive TUI with an RTL-SDR dongle
ship162 --interactive rtlsdr://

# JSON output from the Norwegian Coastal Administration free feed
ship162 --verbose tcp://153.44.253.27:5631

# Write to a file while also displaying the TUI
ship162 --interactive --output ais.jsonl rtlsdr://
```

The centered controls bar follows Jet1090's shortcut notation:
`(Esc/Q) quit | (↑/K) up | (↓/J) down | (⤒/G) top | (S) sources | (E) errors`.
Lowercase keys work too. `G`, Home, or PageUp returns the active view to the top.
Press `S` to show or hide source details; Esc quits unless the error window is open. Like Jet1090, the table border
shows `●` connected (green), `○` connecting (cyan), `↻` reconnecting (yellow),
and `✕` failed (red), including zero counts. Finished recordings use `✓` and
stopped sources use `■`. Every configured source is counted from startup,
including MQTT sources that have not received an AIS message yet.

Press `E` (or `e`) to toggle recent warnings and errors. While the window is
open, arrows or `J`/`K` scroll its history, `G` returns to the newest entry,
and Esc closes it. The controls bar changes to describe the active window. The `⚠` badge counts unread errors. Up to
100 events are retained, repeated consecutive diagnostics are grouped, and
opening the window marks errors read without deleting them. After quitting,
the retained diagnostics are printed to stderr, outside the restored TUI.
The `s` panel separately shows each source's current status and failure reason.

## Configuration file

Settings and sources can be stored in a TOML file. ship162 looks for configuration in order:

1. `$SHIP162_CONFIG` (environment variable)
2. `$XDG_CONFIG_HOME/ship162/config.toml`
3. `~/.config/ship162/config.toml`

```toml
interactive = true

[[sources]]
rtlsdr = { device = 0 }
gain = 49.6

[[sources]]
tcp = "153.44.253.27:5631"
```

See [`config.toml.example`](config.toml.example) for the full reference with all sources and options.

## Sources

### SDR hardware

All SDR sources accept `gain`, `sample_rate`, and `bias_tee` at the source level. If `gain` is omitted, ship162 applies its AIS per-device default; `gain = "auto"` explicitly requests device automatic gain control where supported:

```toml
# RTL-SDR (default gain 49.6 dB)
[[sources]]
rtlsdr = { device = 0 }
gain = 49.6
bias_tee = false

# Airspy R2 or Mini (default gain 50, sensitivity mode)
[[sources]]
airspy = { device = 0 }
gain = 50
sample_rate = 6000000

# HackRF (default LNA=40 dB, VGA=55 dB)
[[sources]]
hackrf = { device = 0, amp_enable = true }

# Airspy Mini via SoapySDR at 3 MS/s
[[sources]]
soapy = "driver=airspy"
sample_rate = 3000000
gain = 49.6
```

For RTL-SDR, `sample_rate` may also be included in the device table:

```toml
[[sources]]
rtlsdr = { device = 0, sample_rate = 2304000 }
```

This is equivalent to putting `sample_rate = 2304000` beside `rtlsdr`. If both
forms are present, their values must match; conflicting rates are an error.
With the optional `soapy` feature, structured device arguments and a nested
sample rate are also accepted:

```toml
[[sources]]
soapy = { args = "driver=airspy", sample_rate = 3000000 }
gain = { LNA = 13, MIX = 3, VGA = 13 }
```

This is equivalent to `soapy = "driver=airspy"` with a source-level sample rate.
Conflicting nested and source-level rates are rejected for either backend.
These forms do not enable optional backends: SoapySDR still requires
`--features soapy` (not in prebuilt binaries), and MQTT requires `--features mqtt`
when building from source (prebuilt binaries include it).
Unsupported backends and malformed settings are reported by source/field name,
rather than a generic untagged-enum error.

SDR receivers retry opening after an error or disconnection, with a fixed
five-second delay. An unplugged receiver therefore does not stop other sources
or close the TUI. Quitting interrupts the retry wait and stops/joins active IQ
workers before exit. Invalid sample rates are rejected before starting sources.

### IQ recordings

```toml
[[sources]]
iqfile = "/path/to/recording.iq"
```

The current file configuration expects raw CU8 IQ at 288 kHz. File sources are
finite and are not retried: the application exits when all sources finish.
Missing or truncated captures produce a nonzero exit status, rather than being
silently ignored. When a live source is also configured, it continues receiving.

### TCP

TCP sources reconnect after a fixed five-second delay by default. Configure the delay per source with `retry`:

```toml
[[sources]]
tcp = "153.44.253.27:5631"

# With SSH tunnel (built-in, no openssh needed)
[[sources]]
tcp = { host = "remote-host", port = 5631, jump = "jumphost" }
# optional
retry = { strategy = "fixed", delay_seconds = 10 }
```

### WebSocket

WebSocket sources also use the same fixed-delay retry policy by default:

```toml
[[sources]]
ws = "ws://remote-host:88888"
# optional
retry = { strategy = "fixed", delay_seconds = 10 }
```

WebSocket and TCP sources go through an HTTP proxy when `http_proxy` (or `all_proxy`) is set and the host is not listed in `no_proxy`; see [Proxies](#proxies).

### aisstream.io

[aisstream.io](https://aisstream.io) relays decoded AIS messages from a worldwide network of terrestrial receivers. It needs a free API key, given by one of (first found):

- `key`, the key itself, in the configuration file;
- `key_file`, a file containing only the key (not both `key` and `key_file`);
- the `AISSTREAM_API_KEY` environment variable.

```toml
[[sources]]
[sources.aisstream]
bbox = [[48.5, -6.0], [52.0, 3.0]] # optional: [[latitude, longitude], [latitude, longitude]]
key = "..."                        # or key_file = "~/.config/ship162/aisstream_api_key"
# url = "wss://stream.aisstream.io/v0/stream"  # optional, the default

[[sources]]
# several boxes, optional filters, and a retry delay
aisstream = { bbox = [[[48, -6], [52, 3]], [[54, 7], [58, 13]]], message_types = ["PositionReport", "ShipStaticData"], mmsi = ["227006760"] }
retry = { strategy = "fixed", delay_seconds = 10 }
```

With `key` in it, keep the configuration file private (`chmod 600`). The key is never printed in logs or error messages.

Without `bbox`, the subscription covers the whole world. On the command line, use `aisstream://?bbox=lat,lon,lat,lon` (repeatable) with `AISSTREAM_API_KEY` set:

```sh
AISSTREAM_API_KEY=... ship162 --output channel.jsonl "aisstream://?bbox=48.5,-6,52,3"
```

Position reports (types 1–3, 18, 19) and static data (types 5, 24) are converted to the same messages as the NMEA decoder; other message types are ignored. The connection negotiates `permessage-deflate` compression, as aisstream.io requests, and goes through the HTTPS proxy when `https_proxy` is set. Enabled by default with the `aisstream` feature.

### MQTT

The `vessels-v2/status` service-status topic is received with the wildcard
subscription but is not AIS data. It is ignored without emitting parse warnings;
malformed location/metadata messages still produce diagnostics.

Included in prebuilt binaries; from source, requires `--features mqtt`. Connects to the [Finnish Digitraffic](https://www.digitraffic.fi/en/marine-traffic/) broker by default, over secure websockets. On hosts that only reach the Internet through an HTTP proxy, set `https_proxy` (and `no_proxy` for hosts to reach directly): the connection then goes through a `CONNECT` tunnel.

```toml
[[sources]]
[sources.mqtt]
url = "wss://meri.digitraffic.fi/mqtt" # optional, the default
topic = "vessels-v2/#"                 # optional, the default
client_id = "ship162-myname"           # optional, default: ship162- and a random suffix
```

`mqtt = "ship162-myname"` (the client ID alone) connects to Digitraffic with the default topic. The broker URL accepts `wss://`, `ws://`, `mqtt://` and `tcp://`; messages must follow Digitraffic's `vessels-v2/<mmsi>/location` and `metadata` format. On the command line: `mqtt://?url=…&topic=…&client_id=…`.

### Proxies

On hosts that only reach the Internet through an HTTP proxy, ship162 opens network connections with a `CONNECT` tunnel, reading the same variables as curl:

| Variable                     | Used for                               |
| ---------------------------- | -------------------------------------- |
| `https_proxy`, `HTTPS_PROXY` | aisstream.io, MQTT over `wss://`       |
| `http_proxy`, `HTTP_PROXY`   | `ws://` sources, MQTT over `ws://`     |
| `all_proxy`, `ALL_PROXY`     | fallback for all of them               |
| `no_proxy`, `NO_PROXY`       | comma-separated hosts reached directly |

Only `http://` proxies are supported; `http://user:password@proxy:3128` sends Basic credentials.

## Output

Decoded messages are emitted as JSON on stdout (`--verbose`), written to a file (`--output`), or published to Redis (`--redis-url`). Verbose and interactive modes are mutually exclusive because each requires stdout; `--log-file -` is likewise unavailable with either mode. The application can also re-broadcast decoded NMEA sentences to downstream consumers:

```sh
# Serve NMEA over TCP for other applications (e.g. OpenCPN)
ship162 --serve-tcp 0.0.0.0:5631 rtlsdr://

# Forward to a UDP endpoint
ship162 --serve-udp 0.0.0.0:5632 rtlsdr://
```

### Deduplication

With several sources, the same message is often received more than once: by
two antennas, or through two aggregated feeds relaying the same coastal
stations. ship162 then merges receptions before output, in two stages:

- **exact**: receptions that carry raw AIS bits (TCP, WebSocket, SDR and IQ
  sources) merge when the bits are identical and they are less than
  `deduplication` milliseconds apart (default 1000);
- **soft**: decoded feeds (MQTT) have no raw bits, so a reception from one of
  them merges with another when both report the same vessel at the same
  position (or carry identical content, for other messages) less than
  `soft_deduplication` milliseconds apart (default 3000).

Receptions from the same source are never merged, and a reception that could
belong to more than one pending message is kept apart. Each output message lists
every reception in `metadata`, with its source and timestamp, and keeps the
earliest timestamp. Messages are held for the longer window before output. Use
`--no-deduplication` to pass every reception through; with a single source,
nothing is held.

```toml
deduplication = 1000       # ms, identical raw AIS bits
soft_deduplication = 3000  # ms, decoded feeds
```

## Free AIS data sources

| Source                           | Address                    | Notes                            |
| -------------------------------- | -------------------------- | -------------------------------- |
| Norwegian Coastal Administration | `tcp://153.44.253.27:5631` | IEC 61162-1 NMEA with timestamps |
| Finnish Digitraffic              | `mqtt://`                  | MQTT over secure websockets      |
| aisstream.io                     | `aisstream://?bbox=…`      | Free API key, worldwide          |

## Similar projects

- [AIS-catcher](https://github.com/jvde-github/AIS-catcher) — C++, very comprehensive SDR support
- [pyais](https://github.com/M0r13n/pyais/) — Python decoder
- [ais](https://github.com/squidpickles/ais) / [nmea-parser](https://github.com/zaari/nmea-parser) — Rust decoders

## License

MIT — see [license.md](license.md).
