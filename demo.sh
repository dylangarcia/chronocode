#!/bin/bash
# Demo script to generate a sample filewatcher recording with file contents
# This directly creates a JSON file without running filewatcher

set -e

# Create recordings directory if it doesn't exist
mkdir -p recordings

RECORDING="recordings/demo_recording.json"

echo "=============================================="
echo "  File Watcher Demo - Recording Generator"
echo "=============================================="
echo ""

# Generate the demo recording directly as JSON with file contents
cat > "$RECORDING" << 'JSONEOF'
{
  "start_time": 1700000000,
  "initial_state": [
    {"path": "demo_project", "size": 0, "is_dir": true}
  ],
  "events": [
    {"timestamp": 0.1, "event_type": "created", "path": "demo_project/src", "size": 0, "is_dir": true},
    {"timestamp": 0.2, "event_type": "created", "path": "demo_project/tests", "size": 0, "is_dir": true},
    {"timestamp": 0.3, "event_type": "created", "path": "demo_project/README.md", "size": 45, "is_dir": false, "content": "# Demo Project\n\nA sample Python project."},
    {"timestamp": 0.5, "event_type": "created", "path": "demo_project/src/main.py", "size": 156, "is_dir": false, "content": "#!/usr/bin/env python3\n\"\"\"Main application module.\"\"\"\n\ndef main():\n    \"\"\"Entry point.\"\"\"\n    print(\"Hello, World!\")\n\nif __name__ == \"__main__\":\n    main()"},
    {"timestamp": 0.6, "event_type": "created", "path": "demo_project/config.json", "size": 52, "is_dir": false, "content": "{\n  \"name\": \"demo\",\n  \"version\": \"1.0.0\"\n}"},
    {"timestamp": 0.7, "event_type": "created", "path": "demo_project/src/utils.py", "size": 120, "is_dir": false, "content": "\"\"\"Utility functions.\"\"\"\n\ndef format_name(name: str) -> str:\n    \"\"\"Format a name.\"\"\"\n    return name.strip().title()"},
    {"timestamp": 0.8, "event_type": "created", "path": "demo_project/src/constants.py", "size": 85, "is_dir": false, "content": "\"\"\"Project constants.\"\"\"\n\nAPP_NAME = \"DemoApp\"\nVERSION = \"1.0.0\"\nDEBUG = True"},
    {"timestamp": 1.0, "event_type": "modified", "path": "demo_project/src/main.py", "size": 312, "is_dir": false, "content": "#!/usr/bin/env python3\n\"\"\"Main application module.\"\"\"\n\nfrom utils import format_name\nfrom constants import APP_NAME, VERSION\n\ndef greet(name: str) -> str:\n    \"\"\"Greet a user.\"\"\"\n    formatted = format_name(name)\n    return f\"Hello, {formatted}!\"\n\ndef main():\n    \"\"\"Entry point.\"\"\"\n    print(f\"{APP_NAME} v{VERSION}\")\n    print(greet(\"world\"))\n\nif __name__ == \"__main__\":\n    main()"},
    {"timestamp": 1.1, "event_type": "modified", "path": "demo_project/README.md", "size": 210, "is_dir": false, "content": "# Demo Project\n\nA sample Python project demonstrating the file watcher.\n\n## Installation\n\n```bash\npip install -r requirements.txt\n```\n\n## Usage\n\n```bash\npython src/main.py\n```"},
    {"timestamp": 1.2, "event_type": "created", "path": "demo_project/requirements.txt", "size": 28, "is_dir": false, "content": "requests>=2.28.0\npytest>=7.0.0"},
    {"timestamp": 1.3, "event_type": "modified", "path": "demo_project/config.json", "size": 89, "is_dir": false, "content": "{\n  \"name\": \"demo\",\n  \"version\": \"1.0.0\",\n  \"debug\": true,\n  \"api_url\": \"https://api.example.com\"\n}"},
    {"timestamp": 1.5, "event_type": "modified", "path": "demo_project/src/utils.py", "size": 320, "is_dir": false, "content": "\"\"\"Utility functions.\"\"\"\n\nimport re\nfrom typing import Optional\n\ndef format_name(name: str) -> str:\n    \"\"\"Format a name.\"\"\"\n    return name.strip().title()\n\ndef validate_email(email: str) -> bool:\n    \"\"\"Validate an email address.\"\"\"\n    pattern = r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$'\n    return bool(re.match(pattern, email))\n\ndef slugify(text: str) -> str:\n    \"\"\"Convert text to URL slug.\"\"\"\n    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')"},
    {"timestamp": 1.6, "event_type": "created", "path": "demo_project/tests/test_utils.py", "size": 350, "is_dir": false, "content": "\"\"\"Tests for utility functions.\"\"\"\n\nimport pytest\nfrom src.utils import format_name, validate_email, slugify\n\nclass TestFormatName:\n    def test_basic(self):\n        assert format_name(\"john\") == \"John\"\n    \n    def test_with_spaces(self):\n        assert format_name(\"  jane doe  \") == \"Jane Doe\"\n\nclass TestValidateEmail:\n    def test_valid(self):\n        assert validate_email(\"test@example.com\") is True\n    \n    def test_invalid(self):\n        assert validate_email(\"not-an-email\") is False\n\nclass TestSlugify:\n    def test_basic(self):\n        assert slugify(\"Hello World\") == \"hello-world\""},
    {"timestamp": 1.7, "event_type": "modified", "path": "demo_project/src/main.py", "size": 520, "is_dir": false, "content": "#!/usr/bin/env python3\n\"\"\"Main application module.\"\"\"\n\nimport sys\nfrom utils import format_name, validate_email\nfrom constants import APP_NAME, VERSION, DEBUG\n\ndef greet(name: str) -> str:\n    \"\"\"Greet a user.\"\"\"\n    formatted = format_name(name)\n    return f\"Hello, {formatted}!\"\n\ndef process_user(email: str, name: str) -> dict:\n    \"\"\"Process user information.\"\"\"\n    if not validate_email(email):\n        raise ValueError(f\"Invalid email: {email}\")\n    return {\n        \"email\": email,\n        \"name\": format_name(name),\n        \"greeting\": greet(name)\n    }\n\ndef main():\n    \"\"\"Entry point.\"\"\"\n    print(f\"{APP_NAME} v{VERSION}\")\n    if DEBUG:\n        print(\"Debug mode enabled\")\n    print(greet(\"world\"))\n\nif __name__ == \"__main__\":\n    main()"},
    {"timestamp": 2.0, "event_type": "created", "path": "demo_project/src/api.py", "size": 480, "is_dir": false, "content": "\"\"\"API client module.\"\"\"\n\nimport json\nfrom typing import Any, Optional\n\nclass APIClient:\n    \"\"\"Simple API client.\"\"\"\n    \n    def __init__(self, base_url: str):\n        \"\"\"Initialize the client.\"\"\"\n        self.base_url = base_url.rstrip('/')\n        self.session = None\n    \n    def get(self, endpoint: str) -> dict:\n        \"\"\"Make a GET request.\"\"\"\n        url = f\"{self.base_url}/{endpoint}\"\n        # Placeholder for actual request\n        return {\"status\": \"ok\"}\n    \n    def post(self, endpoint: str, data: dict) -> dict:\n        \"\"\"Make a POST request.\"\"\"\n        url = f\"{self.base_url}/{endpoint}\"\n        # Placeholder for actual request\n        return {\"status\": \"created\", \"data\": data}\n    \n    def close(self):\n        \"\"\"Close the client.\"\"\"\n        if self.session:\n            self.session = None"},
    {"timestamp": 2.2, "event_type": "modified", "path": "demo_project/src/constants.py", "size": 145, "is_dir": false, "content": "\"\"\"Project constants.\"\"\"\n\nAPP_NAME = \"DemoApp\"\nVERSION = \"1.0.0\"\nDEBUG = True\n\n# API Configuration\nAPI_BASE_URL = \"https://api.example.com\"\nAPI_TIMEOUT = 30"},
    {"timestamp": 2.3, "event_type": "modified", "path": "demo_project/README.md", "size": 445, "is_dir": false, "content": "# Demo Project\n\nA sample Python project demonstrating the file watcher.\n\n## Features\n\n- User management\n- API client\n- Email validation\n- Utility functions\n\n## Installation\n\n```bash\npip install -r requirements.txt\n```\n\n## Usage\n\n```bash\npython src/main.py\n```\n\n## Testing\n\n```bash\npytest tests/\n```\n\n## Project Structure\n\n```\ndemo_project/\n  src/\n    main.py      # Entry point\n    api.py       # API client\n    utils.py     # Utilities\n    constants.py # Config\n  tests/\n    test_utils.py\n```"},
    {"timestamp": 2.5, "event_type": "modified", "path": "demo_project/src/main.py", "size": 680, "is_dir": false, "content": "#!/usr/bin/env python3\n\"\"\"Main application module.\"\"\"\n\nimport sys\nfrom utils import format_name, validate_email\nfrom constants import APP_NAME, VERSION, DEBUG, API_BASE_URL\nfrom api import APIClient\n\ndef greet(name: str) -> str:\n    \"\"\"Greet a user.\"\"\"\n    formatted = format_name(name)\n    return f\"Hello, {formatted}!\"\n\ndef process_user(email: str, name: str) -> dict:\n    \"\"\"Process user information.\"\"\"\n    if not validate_email(email):\n        raise ValueError(f\"Invalid email: {email}\")\n    return {\n        \"email\": email,\n        \"name\": format_name(name),\n        \"greeting\": greet(name)\n    }\n\ndef fetch_data():\n    \"\"\"Fetch data from API.\"\"\"\n    client = APIClient(API_BASE_URL)\n    try:\n        return client.get(\"data\")\n    finally:\n        client.close()\n\ndef main():\n    \"\"\"Entry point.\"\"\"\n    print(f\"{APP_NAME} v{VERSION}\")\n    if DEBUG:\n        print(\"Debug mode enabled\")\n    print(greet(\"world\"))\n    data = fetch_data()\n    print(f\"Fetched: {data}\")\n\nif __name__ == \"__main__\":\n    main()"},
    {"timestamp": 2.8, "event_type": "created", "path": "demo_project/src/models.py", "size": 420, "is_dir": false, "content": "\"\"\"Data models.\"\"\"\n\nfrom dataclasses import dataclass\nfrom typing import Optional\nfrom datetime import datetime\n\n@dataclass\nclass User:\n    \"\"\"User model.\"\"\"\n    id: int\n    email: str\n    name: str\n    created_at: datetime\n    \n    def to_dict(self) -> dict:\n        \"\"\"Convert to dictionary.\"\"\"\n        return {\n            \"id\": self.id,\n            \"email\": self.email,\n            \"name\": self.name,\n            \"created_at\": self.created_at.isoformat()\n        }\n\n@dataclass\nclass APIResponse:\n    \"\"\"API response wrapper.\"\"\"\n    status: str\n    data: Optional[dict] = None\n    error: Optional[str] = None"},
    {"timestamp": 3.0, "event_type": "modified", "path": "demo_project/src/api.py", "size": 720, "is_dir": false, "content": "\"\"\"API client module.\"\"\"\n\nimport json\nfrom typing import Any, Optional\nfrom models import User, APIResponse\n\nclass APIClient:\n    \"\"\"Simple API client.\"\"\"\n    \n    def __init__(self, base_url: str):\n        \"\"\"Initialize the client.\"\"\"\n        self.base_url = base_url.rstrip('/')\n        self.session = None\n        self._cache = {}\n    \n    def get(self, endpoint: str, use_cache: bool = True) -> APIResponse:\n        \"\"\"Make a GET request.\"\"\"\n        url = f\"{self.base_url}/{endpoint}\"\n        if use_cache and url in self._cache:\n            return self._cache[url]\n        # Placeholder for actual request\n        response = APIResponse(status=\"ok\", data={})\n        if use_cache:\n            self._cache[url] = response\n        return response\n    \n    def post(self, endpoint: str, data: dict) -> APIResponse:\n        \"\"\"Make a POST request.\"\"\"\n        url = f\"{self.base_url}/{endpoint}\"\n        # Placeholder for actual request\n        return APIResponse(status=\"created\", data=data)\n    \n    def get_user(self, user_id: int) -> Optional[User]:\n        \"\"\"Get a user by ID.\"\"\"\n        response = self.get(f\"users/{user_id}\")\n        if response.data:\n            return User(**response.data)\n        return None\n    \n    def clear_cache(self):\n        \"\"\"Clear the response cache.\"\"\"\n        self._cache.clear()\n    \n    def close(self):\n        \"\"\"Close the client.\"\"\"\n        self.clear_cache()\n        if self.session:\n            self.session = None"}
  ]
}
JSONEOF

echo "Recording saved to: $RECORDING"
echo ""
echo "Events: 20"
echo "Initial files: 1"
echo "Files with content: All text files include full content"
echo ""
echo "To view the recording:"
echo "  1. Open replay.html in your browser"
echo "  2. Drop the $RECORDING file onto the page"
echo "  3. Use Space to play/pause, arrow keys to step"
echo "  4. Click on a file and use the Structure tab to see code structure"
echo ""

# Optionally open the viewer
if command -v open &> /dev/null; then
    read -p "Open replay.html in browser? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        open replay.html
    fi
fi
