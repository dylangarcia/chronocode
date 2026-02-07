#!/usr/bin/env python3
"""
File Watcher - Watch directory structure and file changes in real-time.

Shows:
- Directory tree structure
- File sizes
- New/modified/deleted files and folders
- Real-time updates
"""

import os
import sys
import time
import json
import argparse
import fnmatch
from pathlib import Path
from datetime import datetime
from typing import Dict, Set, Tuple, Optional, List
from dataclasses import dataclass, field, asdict
from collections import defaultdict
from enum import Enum

try:
    from watchdog.observers import Observer
    from watchdog.events import FileSystemEventHandler, FileSystemEvent
except ImportError:
    print("Error: watchdog library required. Install with: pip install watchdog")
    sys.exit(1)


class EventType(Enum):
    CREATED = "created"
    MODIFIED = "modified"
    DELETED = "deleted"


# ANSI color codes
COLORS = {
    "reset": "\033[0m",
    "bold": "\033[1m",
    "dim": "\033[2m",
    "red": "\033[91m",
    "green": "\033[92m",
    "yellow": "\033[93m",
    "blue": "\033[94m",
    "magenta": "\033[95m",
    "cyan": "\033[96m",
    "white": "\033[97m",
}

# Fun emojis for different file types
FILE_EMOJIS = {
    "dir": "📁",
    "python": "🐍",
    "javascript": "📜",
    "typescript": "📘",
    "json": "📋",
    "markdown": "📝",
    "text": "📄",
    "image": "🖼️ ",
    "video": "🎬",
    "audio": "🎵",
    "archive": "📦",
    "code": "💻",
    "config": "⚙️ ",
    "git": "🔀",
    "lock": "🔒",
    "test": "🧪",
    "doc": "📚",
    "default": "📄",
}

CHANGE_EMOJIS = {
    "created": "✨",
    "modified": "✏️ ",
    "deleted": "🗑️ ",
}

# UI Layout Constants - Fixed width columns for consistent alignment
NAME_WIDTH = 42  # Width of name column (includes emoji)
STATUS_WIDTH = 10  # Width of status column
SIZE_WIDTH = 10  # Width of size column
DELTA_WIDTH = 10  # Width of delta columns
LOC_WIDTH = 8  # Width of LOC column


def get_file_emoji(filename: str, is_dir: bool = False) -> str:
    """Get appropriate emoji for file type."""
    if is_dir:
        return FILE_EMOJIS["dir"]

    ext = filename.lower().split(".")[-1] if "." in filename else ""

    emoji_map = {
        "py": "python",
        "js": "javascript",
        "ts": "typescript",
        "jsx": "javascript",
        "tsx": "typescript",
        "json": "json",
        "md": "markdown",
        "txt": "text",
        "png": "image",
        "jpg": "image",
        "jpeg": "image",
        "gif": "image",
        "svg": "image",
        "mp4": "video",
        "mp3": "audio",
        "zip": "archive",
        "tar": "archive",
        "gz": "archive",
        "yaml": "config",
        "yml": "config",
        "toml": "config",
        "ini": "config",
        "conf": "config",
        "gitignore": "git",
        "lock": "lock",
        "test": "test",
        "pdf": "doc",
        "doc": "doc",
        "docx": "doc",
    }

    return FILE_EMOJIS.get(emoji_map.get(ext, "default"), FILE_EMOJIS["default"])


def get_loc(filepath: Path) -> int:
    """Count lines of code in a file."""
    try:
        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
            return sum(1 for _ in f)
    except (IOError, PermissionError):
        return 0


def format_size(size_bytes: int) -> Tuple[str, str]:
    """Format size and return (formatted_string, unit)."""
    if size_bytes == 0:
        return "0 B", "B"

    units = ["B", "KB", "MB", "GB", "TB"]
    size = float(size_bytes)
    unit_idx = 0

    while size >= 1024.0 and unit_idx < len(units) - 1:
        size /= 1024.0
        unit_idx += 1

    if size < 10:
        formatted = f"{size:.1f}"
    else:
        formatted = f"{size:.0f}"

    return f"{formatted}{units[unit_idx]}", units[unit_idx]


def get_size_color(size_bytes: int) -> str:
    """Get color for size based on magnitude."""
    if size_bytes < 1024:
        return "dim"
    elif size_bytes < 1024 * 1024:
        return "cyan"
    elif size_bytes < 1024 * 1024 * 100:
        return "yellow"
    else:
        return "red"


def format_delta(value: int, is_size: bool = True) -> Tuple[str, str]:
    """Format a delta value with color."""
    if value == 0:
        return "", "dim"

    sign = "+" if value > 0 else ""

    if is_size:
        formatted, _ = format_size(abs(value))
        delta_str = f"{sign}{formatted}"
    else:
        # LOC
        delta_str = f"{sign}{value}"

    color = "green" if value > 0 else "red"
    return delta_str, color


# Maximum file size to capture content (100KB)
MAX_CONTENT_SIZE = 100 * 1024

# Text file extensions for content capture
TEXT_EXTENSIONS = {
    "py",
    "js",
    "ts",
    "jsx",
    "tsx",
    "json",
    "md",
    "txt",
    "html",
    "css",
    "yaml",
    "yml",
    "toml",
    "ini",
    "conf",
    "cfg",
    "sh",
    "bash",
    "zsh",
    "xml",
    "svg",
    "sql",
    "rb",
    "go",
    "rs",
    "java",
    "c",
    "cpp",
    "h",
    "hpp",
    "swift",
    "kt",
    "scala",
    "php",
    "pl",
    "pm",
    "r",
    "lua",
    "vim",
    "el",
    "clj",
    "cljs",
    "ex",
    "exs",
    "erl",
    "hrl",
    "hs",
    "ml",
    "mli",
    "fs",
    "fsi",
    "vue",
    "svelte",
    "astro",
    "graphql",
    "gql",
    "proto",
    "dockerfile",
    "makefile",
    "cmake",
    "gradle",
    "pom",
    "env",
    "gitignore",
    "gitattributes",
}


def is_text_file(path: Path) -> bool:
    """Check if a file is likely a text file based on extension."""
    name = path.name.lower()
    # Check exact name matches (like Makefile, Dockerfile)
    if name in {"makefile", "dockerfile", "gemfile", "rakefile", "procfile"}:
        return True
    # Check extension
    ext = name.split(".")[-1] if "." in name else ""
    return ext in TEXT_EXTENSIONS


def read_file_content(path: Path, max_size: int = MAX_CONTENT_SIZE) -> Optional[str]:
    """Read file content if it's a text file and not too large."""
    try:
        if not path.is_file():
            return None
        if path.stat().st_size > max_size:
            return None
        if not is_text_file(path):
            return None
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read()
    except (IOError, PermissionError, OSError):
        return None


@dataclass
class FileEvent:
    """Represents a single file system event."""

    timestamp: float
    event_type: str
    path: str
    size: int = 0
    is_dir: bool = False
    content: Optional[str] = None  # File content for created/modified events

    def to_dict(self) -> dict:
        result = {
            "timestamp": self.timestamp,
            "event_type": self.event_type,
            "path": self.path,
            "size": self.size,
            "is_dir": self.is_dir,
        }
        if self.content is not None:
            result["content"] = self.content
        return result

    @classmethod
    def from_dict(cls, data: dict) -> "FileEvent":
        return cls(
            timestamp=data["timestamp"],
            event_type=data["event_type"],
            path=data["path"],
            size=data.get("size", 0),
            is_dir=data.get("is_dir", False),
            content=data.get("content"),
        )


class EventLogger:
    """Logs file system events for later replay with continuous file writing."""

    def __init__(
        self,
        output_path: Optional[Path] = None,
        root_path: Optional[Path] = None,
        record_content: bool = False,
    ):
        self.events: List[FileEvent] = []
        self.initial_state: List[dict] = []  # Snapshot of files at start
        self.start_time: Optional[float] = None
        self.output_path = output_path
        self.root_path = root_path  # Used to convert absolute paths to relative
        self.record_content = record_content  # Whether to capture file contents

    def _to_relative_path(self, path: Path) -> str:
        """Convert an absolute path to a relative path from root."""
        if self.root_path is None:
            return str(path)
        try:
            rel_path = path.relative_to(self.root_path)
            # Return "." for the root directory itself
            if str(rel_path) == ".":
                return "."
            return str(rel_path)
        except ValueError:
            # Path is not relative to root, return as-is
            return str(path)

    def set_initial_state(self, file_infos: Dict[Path, "FileInfo"]):
        """Capture the initial state of all files."""
        self.initial_state = []
        for path, info in file_infos.items():
            item = {
                "path": self._to_relative_path(path),
                "size": info.size,
                "is_dir": info.is_dir,
            }
            # Capture content for text files (only if enabled)
            if self.record_content and not info.is_dir:
                content = read_file_content(path)
                if content is not None:
                    item["content"] = content
            self.initial_state.append(item)

    def start_recording(self):
        """Start recording events and initialize output file."""
        self.start_time = time.time()
        self.events = []

        # Write initial state to file
        if self.output_path:
            self._write_file()

    def _write_file(self):
        """Write the complete valid JSON file."""
        if not self.output_path:
            return

        data = {
            "start_time": self.start_time,
            "initial_state": self.initial_state,
            "events": [e.to_dict() for e in self.events],
        }

        # Write to temp file first, then rename for atomic operation
        temp_path = self.output_path.with_suffix(".tmp")
        with open(temp_path, "w") as f:
            json.dump(data, f, separators=(",", ":"))

        # Atomic rename
        temp_path.rename(self.output_path)

    def log_event(
        self, event_type: EventType, path: Path, size: int = 0, is_dir: bool = False
    ):
        """Log a file system event and write to file immediately."""
        if self.start_time is None:
            self.start_time = time.time()

        # Capture content for created/modified text files (only if enabled)
        content = None
        if (
            self.record_content
            and event_type in (EventType.CREATED, EventType.MODIFIED)
            and not is_dir
        ):
            content = read_file_content(path)

        event = FileEvent(
            timestamp=time.time() - self.start_time,
            event_type=event_type.value,
            path=self._to_relative_path(path),
            size=size,
            is_dir=is_dir,
            content=content,
        )
        self.events.append(event)

        # Write complete valid JSON file after each event
        if self.output_path:
            self._write_file()

    def finalize(self):
        """Finalize the recording (writes final state)."""
        if self.output_path:
            self._write_file()

    def save_to_file(self, filepath: Path):
        """Save event log to JSON file (for non-continuous mode or overwrite)."""
        data = {
            "start_time": self.start_time,
            "initial_state": self.initial_state,
            "events": [e.to_dict() for e in self.events],
        }
        with open(filepath, "w") as f:
            json.dump(data, f, indent=2)

    @classmethod
    def load_from_file(cls, filepath: Path) -> "EventLogger":
        """Load event log from JSON file."""
        logger = cls()
        with open(filepath, "r") as f:
            data = json.load(f)

        logger.start_time = data.get("start_time")
        logger.initial_state = data.get("initial_state", [])
        logger.events = [FileEvent.from_dict(e) for e in data.get("events", [])]
        return logger

    def get_statistics(self) -> dict:
        """Get statistics about recorded events."""
        if not self.events:
            return {}

        created = len([e for e in self.events if e.event_type == "created"])
        modified = len([e for e in self.events if e.event_type == "modified"])
        deleted = len([e for e in self.events if e.event_type == "deleted"])

        duration = self.events[-1].timestamp if self.events else 0

        return {
            "total_events": len(self.events),
            "created": created,
            "modified": modified,
            "deleted": deleted,
            "duration_seconds": round(duration, 2),
        }


class GitignoreParser:
    """Parser for .gitignore patterns."""

    def __init__(self, root_path: Path):
        self.root_path = root_path
        self.patterns: Dict[Path, List[Tuple[str, bool]]] = {}
        self._load_gitignores()

    def _load_gitignores(self):
        """Load all .gitignore files recursively."""
        # Load root .gitignore
        root_gitignore = self.root_path / ".gitignore"
        if root_gitignore.exists():
            self.patterns[self.root_path] = self._parse_gitignore(root_gitignore)

        # Load nested .gitignore files
        for gitignore_file in self.root_path.rglob(".gitignore"):
            if gitignore_file != root_gitignore:
                dir_path = gitignore_file.parent
                self.patterns[dir_path] = self._parse_gitignore(gitignore_file)

    def _parse_gitignore(self, gitignore_path: Path) -> List[Tuple[str, bool]]:
        """Parse a .gitignore file and return list of (pattern, is_negation) tuples."""
        patterns = []
        try:
            with open(gitignore_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.rstrip("\n\r")
                    # Skip empty lines and comments
                    if not line or line.startswith("#"):
                        continue

                    # Handle negation
                    is_negation = line.startswith("!")
                    if is_negation:
                        line = line[1:]

                    # Remove leading/trailing whitespace
                    line = line.strip()
                    if not line:
                        continue

                    patterns.append((line, is_negation))
        except (IOError, PermissionError):
            pass

        return patterns

    def _match_pattern(self, rel_path: str, pattern: str) -> bool:
        """Check if a relative path matches a gitignore pattern."""
        # Handle directory-specific patterns
        is_dir_pattern = pattern.endswith("/")
        if is_dir_pattern:
            pattern = pattern[:-1]

        # Normalize pattern and path
        pattern = pattern.lstrip("/")

        # Handle patterns with slashes (anchored to root or specific path)
        if "/" in pattern:
            # Pattern contains slash - match from root
            if fnmatch.fnmatch(rel_path, pattern):
                return True
            if fnmatch.fnmatch(rel_path, pattern + "/*"):
                return True
        else:
            # Pattern without slash - match any file/directory name
            parts = rel_path.split("/")
            for part in parts:
                if fnmatch.fnmatch(part, pattern):
                    return True
                # Also match against full path with pattern
                if fnmatch.fnmatch(rel_path, "*/" + pattern):
                    return True

        return False

    def is_ignored(self, path: Path) -> bool:
        """Check if a path should be ignored based on .gitignore rules."""
        try:
            rel_path = path.relative_to(self.root_path)
            rel_path_str = str(rel_path).replace(os.sep, "/")
        except ValueError:
            return False

        # Check patterns from root to deepest directory
        ignored = False

        # Check all applicable .gitignore files
        for dir_path, patterns in self.patterns.items():
            try:
                # Get path relative to this gitignore's directory
                rel_to_dir = path.relative_to(dir_path)
                rel_to_dir_str = str(rel_to_dir).replace(os.sep, "/")
            except ValueError:
                continue

            # Apply patterns in order
            for pattern, is_negation in patterns:
                if self._match_pattern(rel_to_dir_str, pattern):
                    if is_negation:
                        ignored = False
                    else:
                        ignored = True

        return ignored


@dataclass
class FileInfo:
    path: Path
    size: int
    modified: float
    is_dir: bool = False
    loc: int = 0  # Lines of code

    def get_size_human(self) -> str:
        """Convert bytes to human readable format."""
        if self.is_dir:
            return ""
        size = float(self.size)
        for unit in ["B", "KB", "MB", "GB", "TB"]:
            if size < 1024.0:
                return f"{size:.1f}{unit}"
            size /= 1024.0
        return f"{size:.1f}PB"


class StatisticsTracker:
    """Tracks real-time statistics about file system changes."""

    def __init__(self):
        self.session_start = time.time()
        self.total_created = 0
        self.total_modified = 0
        self.total_deleted = 0
        self.current_files = 0
        self.current_dirs = 0
        self.total_bytes = 0
        self.peak_files = 0
        self.peak_dirs = 0
        self.events_per_minute: List[Tuple[float, str]] = []

    def record_event(self, event_type: str, size: int = 0, is_dir: bool = False):
        """Record an event for statistics."""
        current_time = time.time()
        self.events_per_minute.append((current_time, event_type))

        # Clean old events (older than 60 seconds)
        cutoff = current_time - 60
        self.events_per_minute = [
            (t, e) for t, e in self.events_per_minute if t > cutoff
        ]

        if event_type == "created":
            self.total_created += 1
            if is_dir:
                self.current_dirs += 1
                self.peak_dirs = max(self.peak_dirs, self.current_dirs)
            else:
                self.current_files += 1
                self.peak_files = max(self.peak_files, self.current_files)
                self.total_bytes += size
        elif event_type == "deleted":
            self.total_deleted += 1
            if is_dir:
                self.current_dirs -= 1
            else:
                self.current_files -= 1
                self.total_bytes = max(0, self.total_bytes - size)
        elif event_type == "modified":
            self.total_modified += 1

    def get_stats(self) -> dict:
        """Get current statistics."""
        session_duration = time.time() - self.session_start
        recent_events = len(self.events_per_minute)

        return {
            "session_duration": round(session_duration, 1),
            "total_created": self.total_created,
            "total_modified": self.total_modified,
            "total_deleted": self.total_deleted,
            "current_files": self.current_files,
            "current_dirs": self.current_dirs,
            "peak_files": self.peak_files,
            "peak_dirs": self.peak_dirs,
            "events_per_minute": recent_events,
            "total_bytes": self.total_bytes,
        }

    def format_duration(self, seconds: float) -> str:
        """Format duration in human readable format."""
        if seconds < 60:
            return f"{int(seconds)}s"
        elif seconds < 3600:
            return f"{int(seconds // 60)}m {int(seconds % 60)}s"
        else:
            return f"{int(seconds // 3600)}h {int((seconds % 3600) // 60)}m"


class ChangeTracker:
    """Tracks file system changes between snapshots."""

    def __init__(
        self,
        root_path: Path,
        use_gitignore: bool = True,
        show_hidden: bool = False,
        event_logger: Optional[EventLogger] = None,
        stats_tracker: Optional[StatisticsTracker] = None,
    ):
        self.previous_state: Dict[Path, FileInfo] = {}
        self.current_state: Dict[Path, FileInfo] = {}
        self.changes: Dict[str, Set[Path]] = {
            "added": set(),
            "modified": set(),
            "deleted": set(),
        }
        self.root_path = root_path
        self.use_gitignore = use_gitignore
        self.show_hidden = show_hidden
        self.gitignore_parser: Optional[GitignoreParser] = None
        if use_gitignore:
            self.gitignore_parser = GitignoreParser(root_path)
        self.event_logger = event_logger
        self.stats_tracker = stats_tracker

    def _is_hidden_path(self, path: Path) -> bool:
        """Check if any component of the path starts with a dot."""
        for part in path.relative_to(self.root_path).parts:
            if part.startswith("."):
                return True
        return False

    def _is_recordings_path(self, path: Path) -> bool:
        """Check if a path is inside the recordings directory."""
        try:
            rel_path = path.relative_to(self.root_path)
            parts = rel_path.parts
            return len(parts) > 0 and parts[0] == "recordings"
        except ValueError:
            return False

    def scan_directory(self, root_path: Path) -> Dict[Path, FileInfo]:
        """Scan directory and return file info dictionary."""
        state = {}
        if not root_path.exists():
            return state

        # Add root directory itself
        stat = root_path.stat()
        state[root_path] = FileInfo(
            path=root_path, size=0, modified=stat.st_mtime, is_dir=True
        )

        for path in root_path.rglob("*"):
            try:
                if path.is_symlink():
                    continue

                # Skip hidden files/folders unless show_hidden is True
                if not self.show_hidden and self._is_hidden_path(path):
                    continue

                # Skip recordings directory (don't track our own recording files)
                if self._is_recordings_path(path):
                    continue

                # Check gitignore if enabled
                if self.gitignore_parser and self.gitignore_parser.is_ignored(path):
                    continue

                stat = path.stat()
                is_dir = path.is_dir()

                # Count LOC for text files
                loc = 0
                if not is_dir and path.is_file():
                    loc = get_loc(path)

                state[path] = FileInfo(
                    path=path,
                    size=stat.st_size if path.is_file() else 0,
                    modified=stat.st_mtime,
                    is_dir=is_dir,
                    loc=loc,
                )
            except (OSError, PermissionError):
                continue

        return state

    def update(self, root_path: Path):
        """Update state and detect changes."""
        self.previous_state = self.current_state
        self.current_state = self.scan_directory(root_path)

        self.changes["added"] = set(self.current_state.keys()) - set(
            self.previous_state.keys()
        )
        self.changes["deleted"] = set(self.previous_state.keys()) - set(
            self.current_state.keys()
        )

        self.changes["modified"] = set()
        for path in set(self.current_state.keys()) & set(self.previous_state.keys()):
            curr = self.current_state[path]
            prev = self.previous_state[path]
            if curr.size != prev.size or curr.modified != prev.modified:
                self.changes["modified"].add(path)

        # Log events and track statistics
        for path in self.changes["added"]:
            info = self.current_state[path]
            if self.event_logger:
                self.event_logger.log_event(
                    EventType.CREATED, path, info.size, info.is_dir
                )
            if self.stats_tracker:
                self.stats_tracker.record_event("created", info.size, info.is_dir)

        for path in self.changes["deleted"]:
            info = self.previous_state.get(path)
            if info:
                if self.event_logger:
                    self.event_logger.log_event(
                        EventType.DELETED, path, info.size, info.is_dir
                    )
                if self.stats_tracker:
                    self.stats_tracker.record_event("deleted", info.size, info.is_dir)

        for path in self.changes["modified"]:
            info = self.current_state[path]
            if self.event_logger:
                self.event_logger.log_event(
                    EventType.MODIFIED, path, info.size, info.is_dir
                )
            if self.stats_tracker:
                self.stats_tracker.record_event("modified", info.size, info.is_dir)

    def get_change_type(self, path: Path) -> Optional[str]:
        """Get the type of change for a path."""
        if path in self.changes["added"]:
            return "added"
        elif path in self.changes["modified"]:
            return "modified"
        elif path in self.changes["deleted"]:
            return "deleted"
        return None

    def get_size_delta(self, path: Path) -> int:
        """Get the size change for a modified file."""
        if path not in self.changes["modified"]:
            return 0
        prev = self.previous_state.get(path)
        curr = self.current_state.get(path)
        if prev and curr:
            return curr.size - prev.size
        return 0

    def get_loc_delta(self, path: Path) -> int:
        """Get the LOC change for a modified file."""
        if path not in self.changes["modified"]:
            return 0
        prev = self.previous_state.get(path)
        curr = self.current_state.get(path)
        if prev and curr:
            return curr.loc - prev.loc
        return 0


class TreeRenderer:
    """Renders directory tree with file information."""

    def __init__(
        self,
        tracker: ChangeTracker,
        show_hidden: bool = False,
        max_files_per_dir: Optional[int] = None,
        max_depth: Optional[int] = None,
        show_stats: bool = True,
        stats_tracker: Optional[StatisticsTracker] = None,
    ):
        self.tracker = tracker
        self.show_hidden = show_hidden
        self.max_files_per_dir = max_files_per_dir
        self.max_depth = max_depth
        self.show_stats = show_stats
        self.stats_tracker = stats_tracker
        self.tree_chars = {
            "branch": "├── ",
            "last_branch": "└── ",
            "vertical": "│   ",
            "empty": "    ",
        }
        self.truncation_info = {"files": 0, "dirs": 0, "depth": 0}

    def _color(self, text: str, color: str) -> str:
        """Apply color to text."""
        return f"{COLORS.get(color, '')}{text}{COLORS['reset']}"

    def _format_size(self, size: int, is_dir: bool) -> str:
        """Format file size for display."""
        if is_dir:
            return ""

        formatted, unit = format_size(size)
        color = get_size_color(size)
        return self._color(formatted, color)

    def _should_include(self, path: Path) -> bool:
        """Check if path should be included in tree."""
        if not self.show_hidden and path.name.startswith("."):
            return False
        return True

    def _visible_width(self, text: str) -> int:
        """Calculate visible width of text, ignoring ANSI codes and handling emoji width."""
        import re
        import unicodedata

        # Remove ANSI escape codes
        clean_text = re.sub(r"\033\[[0-9;]*m", "", text)

        width = 0
        i = 0
        while i < len(clean_text):
            char = clean_text[i]
            code = ord(char)

            # Skip variation selectors (often follow emojis)
            if 0xFE00 <= code <= 0xFE0F:
                i += 1
                continue

            # Skip zero-width joiners
            if code == 0x200D:
                i += 1
                continue

            # Emoji ranges (simplified but covers most cases)
            if (
                0x1F300 <= code <= 0x1F9FF  # Misc symbols, emoticons, etc.
                or 0x2600 <= code <= 0x26FF  # Misc symbols
                or 0x2700 <= code <= 0x27BF  # Dingbats
                or 0x1F600 <= code <= 0x1F64F  # Emoticons
                or 0x1F680 <= code <= 0x1F6FF
            ):  # Transport/map symbols
                width += 2
            elif unicodedata.east_asian_width(char) in ("F", "W"):
                # Full-width or Wide characters (CJK, etc.)
                width += 2
            else:
                width += 1

            i += 1

        return width

    def _pad_to_width(self, text: str, width: int) -> str:
        """Pad text to a fixed visible width, accounting for ANSI codes and emojis."""
        visible = self._visible_width(text)
        if visible >= width:
            return text
        return text + " " * (width - visible)

    def _format_loc(self, loc: int) -> str:
        """Format LOC count."""
        if loc == 0:
            return ""
        if loc < 1000:
            return f"{loc}L"
        elif loc < 1000000:
            return f"{loc // 1000}kL"
        else:
            return f"{loc // 1000000}ML"

    def render(self, root_path: Path) -> str:
        """Render the directory tree."""
        lines = []
        self.truncation_info = {"files": 0, "dirs": 0, "depth": 0}

        # Header with column labels
        lines.append(self._color(f"📁 {root_path.absolute()}", "bold"))
        lines.append("")

        # Column headers - use consistent widths
        header = (
            self._color("    ", "dim")
            + self._color(self._pad_to_width("Name", NAME_WIDTH), "dim")
            + self._color(self._pad_to_width("Status", STATUS_WIDTH), "dim")
            + self._color(self._pad_to_width("Size", SIZE_WIDTH), "dim")
            + self._color(self._pad_to_width("Delta", DELTA_WIDTH), "dim")
            + self._color(self._pad_to_width("LOC", LOC_WIDTH), "dim")
            + self._color("Delta", "dim")
        )
        lines.append(header)
        total_width = (
            NAME_WIDTH
            + STATUS_WIDTH
            + SIZE_WIDTH
            + DELTA_WIDTH
            + LOC_WIDTH
            + DELTA_WIDTH
            + 4
        )
        lines.append(self._color("─" * total_width, "dim"))

        # Get all paths and build tree
        all_paths = sorted(self.tracker.current_state.keys())

        # Filter paths
        all_paths = [p for p in all_paths if self._should_include(p)]

        if not all_paths:
            lines.append(self._color("  (empty directory)", "dim"))
            return "\n".join(lines)

        # Build tree structure
        def build_tree(
            prefix: str, items: list, current_depth: int, is_last_list: list
        ):
            # Check depth limit
            if self.max_depth is not None and current_depth > self.max_depth:
                for item in items:
                    if item in self.tracker.current_state:
                        if self.tracker.current_state[item].is_dir:
                            self.truncation_info["dirs"] += 1
                        else:
                            self.truncation_info["files"] += 1
                return

            # Separate files and directories
            dirs = [
                item
                for item in items
                if item in self.tracker.current_state
                and self.tracker.current_state[item].is_dir
            ]
            files = [
                item
                for item in items
                if item in self.tracker.current_state
                and not self.tracker.current_state[item].is_dir
            ]

            # Apply max_files_per_dir limit
            truncated_files = []
            if self.max_files_per_dir is not None:
                total_items = len(dirs) + len(files)
                if total_items > self.max_files_per_dir:
                    if len(dirs) >= self.max_files_per_dir:
                        truncated_files = files
                        files = []
                    else:
                        files_to_show = self.max_files_per_dir - len(dirs)
                        truncated_files = files[files_to_show:]
                        files = files[:files_to_show]

                    self.truncation_info["files"] += len(truncated_files)

            # Combine for display (dirs first, then files)
            display_items = dirs + files

            for i, item in enumerate(display_items):
                is_last = i == len(display_items) - 1

                # Determine tree characters
                if is_last:
                    connector = self.tree_chars["last_branch"]
                else:
                    connector = self.tree_chars["branch"]

                # Get file info
                info = self.tracker.current_state.get(item)
                if not info:
                    continue

                # Build the line
                line_parts = []

                # 1. Tree prefix + connector
                tree_part = prefix + connector

                # 2. Emoji + Name
                emoji = get_file_emoji(item.name, info.is_dir)
                name = item.name

                # Truncate name if too long
                max_name_len = NAME_WIDTH - 3  # Account for emoji and space
                if len(name) > max_name_len:
                    name = name[: max_name_len - 3] + "..."

                emoji_name = f"{emoji} {name}"
                name_colored = self._color(emoji_name, "white")

                # 3. Status - use fixed-width status strings
                change_type = self.tracker.get_change_type(item)
                if change_type == "added":
                    status = self._color("NEW", "green")
                elif change_type == "modified":
                    status = self._color("MOD", "yellow")
                elif change_type == "deleted":
                    status = self._color("DEL", "red")
                else:
                    status = ""

                # 4. Size
                size_str = self._format_size(info.size, info.is_dir)

                # 5. Size delta (for modified files)
                size_delta = ""
                size_delta_color = "dim"
                if change_type == "modified":
                    delta = self.tracker.get_size_delta(item)
                    size_delta, size_delta_color = format_delta(delta, is_size=True)

                # 6. LOC
                loc_str = self._format_loc(info.loc) if not info.is_dir else ""

                # 7. LOC delta (for modified files)
                loc_delta = ""
                loc_delta_color = "dim"
                if change_type == "modified" and not info.is_dir:
                    delta = self.tracker.get_loc_delta(item)
                    loc_delta, loc_delta_color = format_delta(delta, is_size=False)

                # Assemble line with fixed columns using constants
                line = (
                    tree_part
                    + self._pad_to_width(emoji_name, NAME_WIDTH)
                    + self._pad_to_width(status, STATUS_WIDTH)
                    + self._pad_to_width(size_str, SIZE_WIDTH)
                    + self._color(
                        self._pad_to_width(size_delta, DELTA_WIDTH), size_delta_color
                    )
                    + self._pad_to_width(loc_str, LOC_WIDTH)
                    + self._color(
                        self._pad_to_width(loc_delta, DELTA_WIDTH), loc_delta_color
                    )
                )

                lines.append(line)

                # Process children
                if info.is_dir:
                    children = [p for p in all_paths if p.parent == item and p != item]
                    if children:
                        new_prefix = prefix + (
                            self.tree_chars["empty"]
                            if is_last
                            else self.tree_chars["vertical"]
                        )
                        build_tree(
                            new_prefix,
                            children,
                            current_depth + 1,
                            is_last_list + [is_last],
                        )

            # Show truncation message if files were hidden
            if truncated_files:
                is_last = True
                connector = self.tree_chars["last_branch"]
                msg = self._color(
                    f"  ... and {len(truncated_files)} more file(s)", "dim"
                )
                lines.append(f"{prefix}{connector}{msg}")

        # Start with root's children
        root_children = [
            p for p in all_paths if p.parent == root_path and p != root_path
        ]
        build_tree("", root_children, 1, [])

        total_width = (
            NAME_WIDTH
            + STATUS_WIDTH
            + SIZE_WIDTH
            + DELTA_WIDTH
            + LOC_WIDTH
            + DELTA_WIDTH
            + 4
        )
        lines.append(self._color("─" * total_width, "dim"))

        # Summary
        lines.append("")
        lines.append(self._render_summary())

        # Show truncation info if applicable
        if self.truncation_info["files"] > 0 or self.truncation_info["dirs"] > 0:
            trunc_parts = []
            if self.truncation_info["files"] > 0:
                trunc_parts.append(f"{self.truncation_info['files']} files")
            if self.truncation_info["dirs"] > 0:
                trunc_parts.append(f"{self.truncation_info['dirs']} dirs")
            lines.append(
                self._color(f"  ⚠️  Hidden: {', '.join(trunc_parts)}", "yellow")
            )

        return "\n".join(lines)

    def _render_summary(self) -> str:
        """Render summary statistics."""
        total_files = sum(
            1 for info in self.tracker.current_state.values() if not info.is_dir
        )
        total_dirs = sum(
            1 for info in self.tracker.current_state.values() if info.is_dir
        )
        total_size = sum(
            info.size for info in self.tracker.current_state.values() if not info.is_dir
        )
        total_loc = sum(
            info.loc for info in self.tracker.current_state.values() if not info.is_dir
        )

        # Format total size
        size_str, _ = format_size(total_size)

        # Format total LOC
        loc_str = self._format_loc(total_loc)

        parts = [
            self._color(f"📊 {total_files} files", "cyan"),
            self._color(f"📂 {total_dirs} dirs", "blue"),
            self._color(f"💾 {size_str}", "green"),
            self._color(f"📄 {loc_str} lines", "magenta"),
        ]

        # Add change counts with emojis
        changes = []
        if self.tracker.changes["added"]:
            changes.append(
                self._color(f"✨ {len(self.tracker.changes['added'])} new", "green")
            )
        if self.tracker.changes["modified"]:
            changes.append(
                self._color(f"✏️  {len(self.tracker.changes['modified'])} mod", "yellow")
            )
        if self.tracker.changes["deleted"]:
            changes.append(
                self._color(f"🗑️  {len(self.tracker.changes['deleted'])} del", "red")
            )

        if changes:
            parts.append(" │ " + " ".join(changes))

        return "  ".join(parts)

    def _render_stats_dashboard(self) -> List[str]:
        """Render a statistics dashboard."""
        if not self.stats_tracker:
            return []

        stats = self.stats_tracker.get_stats()
        lines = []

        # Box drawing characters
        h = "─"
        v = "│"
        tl = "┌"
        tr = "┐"
        bl = "└"
        br = "┘"
        lm = "├"
        rm = "┤"

        width = 68

        # Dashboard header
        title = "📈 DEVELOPMENT STATISTICS"
        title_padding = (width - len(title) - 2) // 2
        lines.append(self._color(f"{tl}{h * (width - 2)}{tr}", "cyan"))
        lines.append(
            self._color(
                f"{v}{' ' * title_padding}{title}{' ' * (width - 2 - title_padding - len(title))}{v}",
                "bold",
            )
        )
        lines.append(self._color(f"{lm}{h * (width - 2)}{rm}", "cyan"))

        # Session info
        duration = self.stats_tracker.format_duration(stats["session_duration"])
        lines.append(
            self._color(f"{v} ⏱️  Session Duration: {duration:<43}{v}", "white")
        )
        lines.append(
            self._color(
                f"{v} ⚡ Activity Rate: {stats['events_per_minute']:<3} events/min{' ' * 21}{v}",
                "white",
            )
        )
        lines.append(self._color(f"{lm}{h * (width - 2)}{rm}", "cyan"))

        # Activity counters - aligned
        created = f"✨ Created: {stats['total_created']:<4}"
        modified = f"✏️  Modified: {stats['total_modified']:<4}"
        deleted = f"🗑️  Deleted: {stats['total_deleted']:<4}"
        lines.append(
            self._color(f"{v} {created}  {modified}  {deleted}{' ' * 9}{v}", "white")
        )
        lines.append(self._color(f"{lm}{h * (width - 2)}{rm}", "cyan"))

        # Current state - aligned
        files = f"📁 Files: {stats['current_files']:<4} / {stats['peak_files']:<4} peak"
        dirs = f"📂 Dirs: {stats['current_dirs']:<4} / {stats['peak_dirs']:<4} peak"
        lines.append(self._color(f"{v} {files}   {dirs}{' ' * 7}{v}", "white"))

        # Footer
        lines.append(self._color(f"{bl}{h * (width - 2)}{br}", "cyan"))

        return lines


class FileWatcher(FileSystemEventHandler):
    """Handles file system events and updates display."""

    def __init__(
        self,
        root_path: Path,
        show_hidden: bool = False,
        refresh_interval: float = 1.0,
        max_files_per_dir: Optional[int] = None,
        max_depth: Optional[int] = None,
        use_gitignore: bool = True,
        show_stats: bool = True,
        event_logger: Optional[EventLogger] = None,
    ):
        self.root_path = root_path
        self.show_hidden = show_hidden
        self.refresh_interval = refresh_interval
        self.max_files_per_dir = max_files_per_dir
        self.max_depth = max_depth
        self.use_gitignore = use_gitignore
        self.show_stats = show_stats
        self.event_logger = event_logger
        self.stats_tracker = StatisticsTracker() if show_stats else None
        self.tracker = ChangeTracker(
            root_path, use_gitignore, show_hidden, event_logger, self.stats_tracker
        )
        self.renderer = TreeRenderer(
            self.tracker,
            show_hidden,
            max_files_per_dir,
            max_depth,
            show_stats,
            self.stats_tracker,
        )
        self.last_update = 0
        self.running = True

    def on_any_event(self, event: FileSystemEvent):
        """Handle any file system event."""
        current_time = time.time()
        if current_time - self.last_update < self.refresh_interval:
            return

        self.last_update = current_time
        self.update_display()

    def update_display(self):
        """Update and display the current state."""
        self.tracker.update(self.root_path)

        # Clear screen
        os.system("clear" if os.name == "posix" else "cls")

        # Print header
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        print(COLORS["bold"] + f"🎬 File Watcher - {timestamp}" + COLORS["reset"])
        print(
            COLORS["dim"]
            + f"📍 Watching: {self.root_path.absolute()}"
            + COLORS["reset"]
        )
        if self.event_logger:
            print(
                COLORS["red"]
                + "🔴 RECORDING"
                + COLORS["reset"]
                + COLORS["dim"]
                + " - Press Ctrl+C to stop"
                + COLORS["reset"]
            )
        else:
            print(COLORS["dim"] + "Press Ctrl+C to stop" + COLORS["reset"])
        print()

        # Print tree
        print(self.renderer.render(self.root_path))
        print()

        # Print statistics dashboard if enabled
        if self.show_stats and self.stats_tracker:
            dashboard = self.renderer._render_stats_dashboard()
            for line in dashboard:
                print(line)
            print()

        print(
            COLORS["dim"] + "Legend: ✨ NEW  ✏️  MODIFIED  🗑️  DELETED" + COLORS["reset"]
        )

    def run(self):
        """Run the file watcher."""
        # Initial scan WITHOUT recording events
        # Temporarily detach the event logger
        event_logger = self.tracker.event_logger
        self.tracker.event_logger = None

        # Do initial scan (this won't log any events)
        self.tracker.update(self.root_path)

        # Capture initial state for recording
        if event_logger:
            event_logger.set_initial_state(self.tracker.current_state)
            event_logger.start_recording()
            # Reattach the logger for future changes
            self.tracker.event_logger = event_logger

        self.update_display()

        # Set up observer
        observer = Observer()
        observer.schedule(self, str(self.root_path), recursive=True)
        observer.start()

        try:
            while self.running:
                time.sleep(0.1)
        except KeyboardInterrupt:
            self.running = False
        finally:
            observer.stop()
            observer.join()
            print("\n" + COLORS["dim"] + "File watcher stopped." + COLORS["reset"])

            # Print final statistics
            if self.stats_tracker:
                stats = self.stats_tracker.get_stats()
                duration = self.stats_tracker.format_duration(stats["session_duration"])
                print(f"\n📊 Session Summary ({duration}):")
                print(f"   ✨ {stats['total_created']} created")
                print(f"   ✏️  {stats['total_modified']} modified")
                print(f"   🗑️  {stats['total_deleted']} deleted")
                print(
                    f"   📁 {stats['current_files']} files, 📂 {stats['current_dirs']} dirs"
                )

            # Finalize event log if recording
            if self.event_logger:
                self.event_logger.finalize()
                print(
                    f"\n🔴 Recording stopped. Total events: {len(self.event_logger.events)}"
                )


def main():
    parser = argparse.ArgumentParser(
        description="Watch file system changes in real-time",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s                              # Watch current directory
  %(prog)s /path/to/dir                 # Watch specific directory
  %(prog)s -a                           # Show hidden files
  %(prog)s -i 0.5                       # Update every 0.5 seconds
  %(prog)s -f 10                        # Show max 10 files per directory
  %(prog)s -d 3                         # Show max 3 levels deep
  %(prog)s -f 5 -d 2                    # Show 5 files per dir, 2 levels deep
  %(prog)s --no-gitignore               # Show ignored files too
  %(prog)s --no-stats                   # Hide statistics dashboard
  %(prog)s -r                           # Record events (auto-named file)
  %(prog)s -r session.json              # Record events to specific file
  %(prog)s --replay session.json .      # Replay recorded session
  %(prog)s --replay session.json -s 2.0 # Replay at 2x speed
        """,
    )

    parser.add_argument(
        "path",
        nargs="?",
        default=".",
        help="Directory to watch (default: current directory)",
    )

    parser.add_argument(
        "-a", "--all", action="store_true", help="Show hidden files and directories"
    )

    parser.add_argument(
        "-i",
        "--interval",
        type=float,
        default=1.0,
        help="Refresh interval in seconds (default: 1.0)",
    )

    parser.add_argument(
        "-f",
        "--max-files",
        type=int,
        default=None,
        metavar="N",
        help="Maximum number of files to show per directory (default: unlimited)",
    )

    parser.add_argument(
        "-d",
        "--max-depth",
        type=int,
        default=None,
        metavar="N",
        help="Maximum depth of directory tree to display (default: unlimited)",
    )

    parser.add_argument(
        "--no-gitignore",
        action="store_true",
        help="Do not respect .gitignore files (show all files)",
    )

    parser.add_argument(
        "--no-stats", action="store_true", help="Disable the statistics dashboard"
    )

    parser.add_argument(
        "-r",
        "--record",
        nargs="?",
        const=True,
        default=False,
        metavar="FILE",
        help="Record events to a JSON file for later replay (default: recording_YYYYMMDD_HHMMSS.json)",
    )

    parser.add_argument(
        "-c",
        "--content",
        action="store_true",
        help="Record file contents in addition to metadata (makes recordings larger)",
    )

    parser.add_argument(
        "--replay",
        type=str,
        metavar="FILE",
        help="Replay events from a previously recorded JSON file",
    )

    parser.add_argument(
        "--replay-speed",
        type=float,
        default=1.0,
        metavar="SPEED",
        help="Replay speed multiplier (default: 1.0, use 2.0 for 2x speed)",
    )

    args = parser.parse_args()

    # Handle replay mode
    if args.replay:
        replay_path = Path(args.replay)
        if not replay_path.exists():
            print(f"Error: Replay file does not exist: {replay_path}")
            sys.exit(1)

        # Load event log
        event_logger = EventLogger.load_from_file(replay_path)
        stats = event_logger.get_statistics()

        print(f"🎬 Loading replay from: {replay_path}")
        print(f"   📊 {stats['total_events']} events over {stats['duration_seconds']}s")
        print(
            f"   ✨ {stats['created']} created, ✏️  {stats['modified']} modified, 🗑️  {stats['deleted']} deleted"
        )
        print(f"   ⏩ Replay speed: {args.replay_speed}x")
        print()

        # Create a mock watcher for replay
        if not args.path:
            print("Error: Please specify the original directory path for replay")
            sys.exit(1)

        root_path = Path(args.path).resolve()
        if not root_path.exists():
            print(f"Error: Path does not exist: {root_path}")
            sys.exit(1)

        # Replay the events
        replay_event_logger = EventLogger()
        replay_event_logger.events = event_logger.events

        watcher = FileWatcher(
            root_path,
            show_hidden=args.all,
            refresh_interval=0.1,  # Fast refresh for replay
            max_files_per_dir=args.max_files,
            max_depth=args.max_depth,
            use_gitignore=not args.no_gitignore,
            show_stats=not args.no_stats,
            event_logger=replay_event_logger,
        )

        # Run replay
        try:
            watcher.tracker.update(root_path)
            watcher.update_display()

            for i, event in enumerate(event_logger.events):
                if i > 0:
                    # Calculate delay from previous event
                    prev_time = event_logger.events[i - 1].timestamp
                    delay = (event.timestamp - prev_time) / args.replay_speed
                    if delay > 0:
                        time.sleep(min(delay, 2.0))  # Cap at 2 seconds max

                # Show the event
                watcher.update_display()

        except KeyboardInterrupt:
            print("\n" + COLORS["dim"] + "Replay stopped." + COLORS["reset"])

        return

    # Normal watch mode
    root_path = Path(args.path).resolve()

    if not root_path.exists():
        print(f"Error: Path does not exist: {root_path}")
        sys.exit(1)

    if not root_path.is_dir():
        print(f"Error: Path is not a directory: {root_path}")
        sys.exit(1)

    # Set up event logger if recording (with continuous file writing)
    event_logger = None
    record_path = None
    if args.record:
        # Create recordings directory
        recordings_dir = Path("recordings")
        recordings_dir.mkdir(exist_ok=True)

        # Generate default filename if none provided
        if args.record is True:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            record_path = recordings_dir / f"recording_{timestamp}.json"
        else:
            # If user provided a path, use it; if just a filename, put it in recordings/
            user_path = Path(args.record)
            if user_path.parent == Path("."):
                record_path = recordings_dir / user_path
            else:
                record_path = user_path
        event_logger = EventLogger(
            output_path=record_path,
            root_path=root_path,
            record_content=args.content,
        )
        content_msg = " with file contents" if args.content else ""
        print(f"🔴 Recording to: {record_path}{content_msg}")

    watcher = FileWatcher(
        root_path,
        show_hidden=args.all,
        refresh_interval=args.interval,
        max_files_per_dir=args.max_files,
        max_depth=args.max_depth,
        use_gitignore=not args.no_gitignore,
        show_stats=not args.no_stats,
        event_logger=event_logger,
    )
    watcher.run()

    # Print save confirmation (file was already written continuously)
    if event_logger and record_path:
        print(f"💾 Saved {len(event_logger.events)} events to: {record_path}")


if __name__ == "__main__":
    main()
