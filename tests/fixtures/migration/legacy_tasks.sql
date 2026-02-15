CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

INSERT INTO tasks(id, title, status, created_at) VALUES
('T_LEGACY_1', 'Legacy open task', 'open', '1700000000Z'),
('T_LEGACY_2', 'Legacy done task', 'done', '1700000100Z');
