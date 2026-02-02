use rusqlite_migration::{Migrations, M};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        "CREATE TABLE sources (
            label           TEXT PRIMARY KEY,
            last_synced_at  TEXT
        );

        CREATE TABLE definitions (
            id              TEXT NOT NULL,
            source_label    TEXT NOT NULL,
            name            TEXT NOT NULL,
            description     TEXT,
            kind            TEXT NOT NULL,
            category        TEXT,
            body            TEXT NOT NULL,
            tools_json      TEXT NOT NULL DEFAULT '[]',
            model           TEXT,
            metadata_json   TEXT NOT NULL DEFAULT '{}',
            raw             TEXT NOT NULL,
            PRIMARY KEY (source_label, id),
            FOREIGN KEY (source_label) REFERENCES sources(label)
        );

        CREATE INDEX idx_definitions_kind ON definitions(kind);
        CREATE INDEX idx_definitions_name ON definitions(name);",
    )])
}
