# chronocode

Record and replay file system changes in real-time. A TUI for watching AI agents (or yourself) build projects.

## Demo

[Demo](https://github.com/user-attachments/assets/902c33b5-9f23-4de2-8195-5ddefc5ac80b)


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

When you exit, you get a shareable command and the viewer opens in your browser:

```
Session summary:
  Duration: 5m 12s
  Events:  23 created, 8 modified, 2 deleted

Recording saved: recordings/recording_20260207_191500.json (33 events)

Share this recording:
  chronocode --load eJy0zTEOgCAQBdDe...
```

## Features

- **Real-time file tree** with emoji icons, sizes, and line counts
- **Change detection** - created, modified, and deleted files highlighted with deltas
- **Statistics dashboard** - session duration, event rate, file/dir counts, activity sparkline, extension breakdown
- **Automatic recording** - every session is saved as JSON for replay
- **Web replay viewer** - timeline scrubbing, content preview, diff view, LOC counts
- **Shareable recordings** - compress a recording and send it as a single command
- **Gitignore support** - respects `.gitignore` by default
- **Worktree support** - automatically discovers and watches git worktrees (great for agentic workflows)
- **Search/filter** - press `/` to search files by name
- **Scrolling** - `j/k`, `g/G`, `PageUp/Down`, `Ctrl+d/u` with position indicator
- **Collapsible folders** in the web viewer

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit |
| `/` | Search/filter files |
| `Esc` | Clear search |
| `j` / `k` | Scroll down / up |
| `g` / `G` | Jump to top / bottom |
| `PageUp` / `PageDown` | Scroll by half page |
| `Ctrl+d` / `Ctrl+u` | Scroll by half page |
| `Ctrl+c` | Force quit |

## Recording

Sessions are recorded automatically to `recordings/` in the watched directory.

```bash
# Disable recording
chronocode --no-record

# Record to a specific file
chronocode -r mysession.json

# Include file contents (enables preview/diff in the viewer)
chronocode -c

# Don't auto-open the viewer when session ends
chronocode --no-open
```

## Replay

### Web viewer

Open the viewer and drop a recording JSON onto it:

```bash
chronocode --viewer
```

Or use the link that auto-opens when a session ends.

Controls: `Space` play/pause, `Arrow keys` step, `R` reset, `P` toggle preview, `S` share, click folders to collapse.

### Terminal replay

```bash
chronocode --replay recordings/session.json
chronocode --replay session.json --replay-speed 3.0
```

### Share a recording

```bash
# Generate a shareable command
chronocode --share recordings/session.json
# Output: chronocode --load eJy0zTEOgCAQBdDe...

# Load a shared recording (opens the web viewer)
chronocode --load eJy0zTEOgCAQBdDe...
```

The `--load` command contains the entire recording (compressed), so the recipient only needs the `chronocode` binary.

### Generate from git history

Create a recording from git commits without running a live session:

```bash
# From a single commit (diff from its parent)
chronocode --git abc123

# From a range of commits
chronocode --git abc123..def456

# From a commit to HEAD
chronocode --git abc123..
```

The recording is saved to `recordings/` and the viewer opens automatically.

## Worktrees

Chronocode automatically discovers and watches git worktrees. This is especially useful for agentic coding workflows where tools like Claude Code spawn worktrees to work in parallel.

When you run `chronocode` inside a git repository, it runs `git worktree list` and watches all worktrees alongside the main directory. Worktree paths are **always** included in the recording, even if they would normally be excluded by `.gitignore` rules (a common setup when the worktrees parent directory is gitignored).

```bash
# Worktrees are watched by default — just run chronocode
chronocode

# Disable worktree watching
chronocode --no-worktrees
```

## Options

```
Usage: chronocode [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to watch [default: .]

Options:
  -a, --all                       Show hidden files and directories
  -i, --interval <SECONDS>        Refresh interval [default: 0.25]
  -f, --max-files <N>             Max files shown per directory
  -d, --max-depth <N>             Max tree depth
      --no-gitignore              Disable gitignore filtering
      --no-stats                  Hide statistics dashboard
      --no-record                 Disable automatic recording
      --no-open                   Don't auto-open the viewer on exit
  -r, --record <FILE>             Record to a specific file
  -c, --content                   Include file contents in recording
      --replay <FILE>             Replay a recorded session
      --replay-speed <SPEED>      Replay speed multiplier [default: 1]
      --viewer                    Open the web replay viewer
      --share <FILE>              Generate a shareable command from a recording
      --load <DATA>               Load a shared recording and open the viewer
      --git <SPEC>                Generate a recording from git commits
      --no-worktrees              Disable watching git worktrees
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
