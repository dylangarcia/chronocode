# chronocode

Record and replay file system changes in real-time. A TUI for watching AI agents (or yourself) build projects.

## Install

```bash
cargo install chronocode
```

Or download a prebuilt binary from [Releases](https://github.com/dylangarcia/chronocode/releases).

## Usage

```bash
# Watch the current directory
chronocode

# Watch a specific directory
chronocode /path/to/project

# Show hidden files, update every 500ms
chronocode -a -i 0.5
```

Press `q` to quit. Every session is automatically recorded.

When you exit, you get a clickable link to replay the session in your browser:

```
Session summary:
  Duration: 5m 12s
  Events:  23 created, 8 modified, 2 deleted

Recording saved: recordings/recording_20260207_191500.json (33 events)

  Open recording: file:///path/to/replay.html#data=...
```

## Features

- **Real-time file tree** with emoji icons, sizes, and line counts
- **Change detection** - created, modified, and deleted files highlighted with deltas
- **Statistics dashboard** - session duration, event rate, file/dir counts
- **Automatic recording** - every session is saved as JSON for replay
- **Web replay viewer** - timeline scrubbing, content preview, diff view, code structure analysis
- **Shareable URLs** - compress a recording into a URL and send it to anyone
- **Gitignore support** - respects `.gitignore` by default
- **Collapsible folders** in the web viewer

## Recording

Sessions are recorded automatically to `recordings/` in the watched directory.

```bash
# Disable recording
chronocode --no-record

# Record to a specific file
chronocode -r mysession.json

# Include file contents (enables preview/diff in the viewer)
chronocode -c
```

## Replay

### Web viewer

Open `replay.html` in your browser and drop a recording JSON onto it. Or use the link printed when a session ends.

Controls: `Space` play/pause, `Arrow keys` step, `R` reset, `P` toggle preview, `S` share, click folders to collapse.

### Terminal replay

```bash
chronocode --replay recordings/session.json
chronocode --replay session.json --replay-speed 3.0
```

### Share a recording

```bash
# Generate a shareable URL
chronocode --share recordings/session.json

# With a custom base URL (e.g. if you host replay.html)
chronocode --share session.json --share-base-url "https://mysite.com/replay.html"
```

The URL contains the entire recording (compressed), so the recipient doesn't need the JSON file.

## Options

```
Usage: chronocode [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to watch [default: .]

Options:
  -a, --all                       Show hidden files and directories
  -i, --interval <SECONDS>        Refresh interval [default: 1]
  -f, --max-files <N>             Max files shown per directory
  -d, --max-depth <N>             Max tree depth
      --no-gitignore              Disable gitignore filtering
      --no-stats                  Hide statistics dashboard
      --no-record                 Disable automatic recording
  -r, --record <FILE>             Record to a specific file
  -c, --content                   Include file contents in recording
      --replay <FILE>             Replay a recorded session
      --replay-speed <SPEED>      Replay speed multiplier [default: 1]
      --viewer                    Open the web replay viewer
      --share <FILE>              Generate a shareable URL from a recording
      --share-base-url <URL>      Base URL for share links
  -V, --version                   Print version
  -h, --help                      Print help
```

## Building from source

```bash
git clone https://github.com/dylangarcia/chronocode.git
cd chronocode
cargo build --release
```

The binary is at `target/release/chronocode`.

## License

MIT
