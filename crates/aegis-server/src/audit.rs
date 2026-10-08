//! Hash-chained audit log (docs/api.md "Audit log").
//! `hash = hex(sha256(prev_hash || canonical_json(payload) || action || entity_id))`.

use futures_util::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// Arbitrary constant key for `pg_advisory_xact_lock`; serialises chain appends.
const LOCK_KEY: i64 = 0x4145_4749_5341_5544; // "AEGISAUD"

/// Compact JSON with object keys sorted recursively.
pub fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical_json(&m[k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical_json).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

pub fn hash(prev_hash: &str, payload: &Value, action: &str, entity_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(canonical_json(payload).as_bytes());
    h.update(action.as_bytes());
    h.update(entity_id.as_bytes());
    hex::encode(h.finalize())
}

pub struct Entry {
    pub actor: String,
    pub action: &'static str,
    pub entity_id: String,
    pub payload: Value,
}

impl Entry {
    pub fn new(actor: &str, action: &'static str, entity_id: impl ToString, payload: Value) -> Self {
        Self { actor: actor.into(), action, entity_id: entity_id.to_string(), payload }
    }
}

/// Append entries in order. `conn` MUST be inside a transaction: the advisory
/// xact lock is held until commit so concurrent appenders cannot fork the chain.
pub async fn append(conn: &mut PgConnection, entries: Vec<Entry>) -> sqlx::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(LOCK_KEY).execute(&mut *conn).await?;
    let mut prev: String = sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or_else(|| GENESIS.into());

    let n = entries.len();
    let (mut actors, mut actions, mut ids, mut payloads, mut prevs, mut hashes) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    for e in entries {
        let h = hash(&prev, &e.payload, e.action, &e.entity_id);
        actors.push(e.actor);
        actions.push(e.action.to_string());
        ids.push(e.entity_id);
        payloads.push(e.payload);
        prevs.push(std::mem::replace(&mut prev, h.clone()));
        hashes.push(h);
    }
    sqlx::query(
        "INSERT INTO audit_log (actor, action, entity_id, payload, prev_hash, hash)
         SELECT a, b, c, d, e, f
         FROM UNNEST($1::text[], $2::text[], $3::text[], $4::jsonb[], $5::text[], $6::text[])
              WITH ORDINALITY AS u(a, b, c, d, e, f, n)
         ORDER BY n",
    )
    .bind(actors)
    .bind(actions)
    .bind(ids)
    .bind(payloads)
    .bind(prevs)
    .bind(hashes)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Verify {
    pub ok: bool,
    pub entries: i64,
    pub broken_at: Option<i64>,
}

/// Recompute the whole chain; `broken_at` = seq of the first row that does not verify.
pub async fn verify(pool: &PgPool) -> sqlx::Result<Verify> {
    let mut rows = sqlx::query_as::<_, (i64, String, String, Value, String, String)>(
        "SELECT seq, action, entity_id, payload, prev_hash, hash FROM audit_log ORDER BY seq",
    )
    .fetch(pool);
    let (mut prev, mut entries, mut broken_at) = (GENESIS.to_string(), 0i64, None);
    while let Some((seq, action, entity_id, payload, prev_hash, h)) = rows.try_next().await? {
        entries += 1;
        if broken_at.is_none() && (prev_hash != prev || hash(&prev, &payload, &action, &entity_id) != h) {
            broken_at = Some(seq);
        }
        prev = h;
    }
    Ok(Verify { ok: broken_at.is_none(), entries, broken_at })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_sorts_keys_recursively() {
        let v = json!({"z": 1, "a": {"y": [1, {"c": 2, "b": "x"}], "b": null}});
        assert_eq!(canonical_json(&v), r#"{"a":{"b":null,"y":[1,{"b":"x","c":2}]},"z":1}"#);
    }

    #[test]
    fn hash_is_sha256_hex_of_concatenation() {
        let h = hash(GENESIS, &json!({}), "a", "e");
        let mut d = Sha256::new();
        d.update(format!("{GENESIS}{{}}ae"));
        assert_eq!(h, hex::encode(d.finalize()));
    }
}
