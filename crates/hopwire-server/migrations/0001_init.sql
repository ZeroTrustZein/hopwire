-- AegisGraph server schema. Append-only: transactions, audit_log.

CREATE TABLE transactions (
    seq        BIGSERIAL UNIQUE,        -- insertion order (replay tiebreak)
    tx_id      TEXT PRIMARY KEY,
    ts_ms      BIGINT NOT NULL,
    debtor     TEXT NOT NULL,
    creditor   TEXT NOT NULL,
    raw        JSONB NOT NULL,          -- full aegis_core::Txn
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX transactions_ts_ms ON transactions (ts_ms);
CREATE INDEX transactions_debtor ON transactions (debtor);
CREATE INDEX transactions_creditor ON transactions (creditor);

CREATE TABLE cases (
    id         UUID PRIMARY KEY,
    title      TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'in_review', 'closed')),
    assignee   TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cases_status ON cases (status);

CREATE TABLE alerts (
    seq          BIGSERIAL UNIQUE,      -- stable "newest first" ordering
    id           UUID PRIMARY KEY,
    tx_id        TEXT NOT NULL REFERENCES transactions (tx_id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    ts_ms        BIGINT NOT NULL,
    score        DOUBLE PRECISION NOT NULL,
    decision     TEXT NOT NULL,
    rules        TEXT[] NOT NULL,
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'escalated', 'closed')),
    debtor       TEXT NOT NULL,
    creditor     TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency     TEXT NOT NULL,
    case_id      UUID REFERENCES cases (id),
    findings     JSONB NOT NULL         -- aegis_core::Finding[]
);
CREATE INDEX alerts_status ON alerts (status);
CREATE INDEX alerts_rules ON alerts USING GIN (rules);
CREATE INDEX alerts_case_id ON alerts (case_id);

CREATE TABLE dispositions (
    alert_id   UUID PRIMARY KEY REFERENCES alerts (id),
    label      TEXT NOT NULL CHECK (label IN ('true_positive', 'false_positive')),
    analyst    TEXT NOT NULL,
    note       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE case_notes (
    id         UUID PRIMARY KEY,
    case_id    UUID NOT NULL REFERENCES cases (id),
    author     TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX case_notes_case_id ON case_notes (case_id);

-- hash = hex(sha256(prev_hash || canonical_json(payload) || action || entity_id)); never updated/deleted.
CREATE TABLE audit_log (
    seq       BIGSERIAL PRIMARY KEY,
    at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor     TEXT NOT NULL,
    action    TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    payload   JSONB NOT NULL,
    prev_hash TEXT NOT NULL,
    hash      TEXT NOT NULL
);
