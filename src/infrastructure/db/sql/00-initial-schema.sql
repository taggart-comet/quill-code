CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    project_root TEXT,
    created_at TEXT NOT NULL,
    session_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    history_from_request_id INTEGER,
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,
    gguf_file_path TEXT,
    date_added TEXT NOT NULL,
    model_name TEXT,
    auth_type TEXT NOT NULL DEFAULT 'local'
);

CREATE TABLE IF NOT EXISTS user_settings (
    id INTEGER PRIMARY KEY,
    openai_api_key TEXT,
    openai_tracing_enabled INTEGER NOT NULL DEFAULT 0,
    use_behavior_trees INTEGER NOT NULL DEFAULT 0,
    current_model_id INTEGER,
    web_search_enabled INTEGER NOT NULL DEFAULT 0,
    brave_api_key TEXT,
    max_tool_calls_per_request INTEGER NOT NULL DEFAULT 50,
    auth_method TEXT NOT NULL DEFAULT 'api_key',
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expiry INTEGER,
    oauth_account_id TEXT,
    FOREIGN KEY (current_model_id) REFERENCES models(id)
);

CREATE TABLE IF NOT EXISTS session_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    prompt TEXT NOT NULL,
    result_summary TEXT,
    file_changes TEXT,
    mode TEXT NOT NULL DEFAULT 'build',
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name TEXT NOT NULL,
    command_pattern TEXT,
    resource_pattern TEXT,
    decision TEXT NOT NULL,
    scope TEXT NOT NULL,
    project_id INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE INDEX IF NOT EXISTS idx_permissions_tool ON permissions(tool_name);
CREATE INDEX IF NOT EXISTS idx_permissions_project ON permissions(project_id);
CREATE INDEX IF NOT EXISTS idx_permissions_scope ON permissions(scope);

CREATE TABLE IF NOT EXISTS todo_lists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL UNIQUE,
    content TEXT NOT NULL DEFAULT '{"items":[]}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS session_request_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id INTEGER NOT NULL,
    step_index INTEGER NOT NULL,
    step_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (request_id) REFERENCES session_requests(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_request_steps_request ON session_request_steps(request_id);