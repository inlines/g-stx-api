-- Your SQL goes here
CREATE TABLE IF NOT EXISTS companies (
    id          INTEGER PRIMARY KEY      NOT NULL,
    changed_company_id  INTEGER,
    start_date INTEGER,
    start_date_format BIGINT,
    status INTEGER,
    name TEXT,
    description TEXT,
    developed TEXT,
    published TEXT
);-- Your SQL goes here
