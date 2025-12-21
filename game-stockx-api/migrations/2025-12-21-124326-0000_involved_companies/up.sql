-- Your SQL goes here
CREATE TABLE IF NOT EXISTS involved_companies (
    id          INTEGER PRIMARY KEY      NOT NULL,
    company  INTEGER,
    game INTEGER,
    developer BOOLEAN,
    porting BOOLEAN,
    publisher BOOLEAN,
    supporting BOOLEAN
);
