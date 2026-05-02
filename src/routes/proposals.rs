use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    models::{AddDateOption, CreateProposal, Proposal, ProposalDateOption, VoteRequest},
    store::Store,
};

const VALID_ACT: &[&str] = &["hike", "via_ferrata", "ski", "trail_run", "cycling", "camping", "other"];

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fetch_proposal(conn: &rusqlite::Connection, id: &str, my_uid: &str) -> Option<Proposal> {
    let row = conn.query_row(
        "SELECT p.id, p.title, p.description, p.activity_type,
                p.created_by, COALESCE(u.display_name, u.username) as cbn,
                p.created_at, p.voting_closes_at, p.status, p.calendar_event_id
         FROM proposals p JOIN users u ON p.created_by = u.id
         WHERE p.id = ?1",
        rusqlite::params![id],
        |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?,
            r.get::<_, String>(4)?, r.get::<_, String>(5)?,
            r.get::<_, String>(6)?, r.get::<_, String>(7)?,
            r.get::<_, String>(8)?, r.get::<_, Option<String>>(9)?,
        )),
    ).ok()?;

    let (pid, title, description, activity_type, created_by, created_by_name,
         created_at, voting_closes_at, status, calendar_event_id) = row;

    let mut stmt = conn.prepare(
        "SELECT pdo.id, pdo.date, pdo.suggested_by,
                COALESCE(u.display_name, u.username),
                COUNT(pv.user_id) as vote_count
         FROM proposal_date_options pdo
         JOIN users u ON pdo.suggested_by = u.id
         LEFT JOIN proposal_votes pv ON pv.date_option_id = pdo.id
         WHERE pdo.proposal_id = ?1
         GROUP BY pdo.id
         ORDER BY vote_count DESC, pdo.date ASC",
    ).ok()?;

    let date_options: Vec<ProposalDateOption> = stmt.query_map(
        rusqlite::params![&pid],
        |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
        )),
    ).ok()?
    .filter_map(|r| r.ok())
    .map(|(oid, date, sug_by, sug_name, vote_count)| {
        let i_voted = conn.query_row(
            "SELECT 1 FROM proposal_votes WHERE proposal_id=?1 AND user_id=?2 AND date_option_id=?3",
            rusqlite::params![&pid, my_uid, &oid],
            |_| Ok(true),
        ).unwrap_or(false);
        ProposalDateOption { id: oid, date, suggested_by: sug_by, suggested_by_name: sug_name, vote_count, i_voted }
    })
    .collect();

    let total_votes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM proposal_votes WHERE proposal_id = ?1",
        rusqlite::params![&pid], |r| r.get(0),
    ).unwrap_or(0);

    Some(Proposal {
        id: pid, title, description, activity_type, created_by, created_by_name,
        created_at, voting_closes_at, status, calendar_event_id,
        date_options, total_votes,
    })
}

/// Auto-close expired voting and create calendar events for the winner.
fn auto_resolve(conn: &rusqlite::Connection, my_uid: &str) -> Vec<Proposal> {
    let now = Utc::now().to_rfc3339();

    // Find expired open proposals
    let expired_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM proposals WHERE status='voting' AND voting_closes_at <= ?1"
        ).unwrap();
        stmt.query_map(rusqlite::params![now], |r| r.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };

    for pid in &expired_ids {
        // Find winning date option
        let winner = conn.query_row(
            "SELECT pdo.id, pdo.date, COUNT(pv.user_id) as cnt
             FROM proposal_date_options pdo
             LEFT JOIN proposal_votes pv ON pv.date_option_id = pdo.id
             WHERE pdo.proposal_id = ?1
             GROUP BY pdo.id ORDER BY cnt DESC, pdo.date ASC LIMIT 1",
            rusqlite::params![pid],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        ).ok();

        if let Some((_opt_id, winning_date, vote_cnt)) = winner {
            if vote_cnt > 0 {
                // Fetch proposal details for the calendar event
                let details = conn.query_row(
                    "SELECT title, activity_type, created_by FROM proposals WHERE id=?1",
                    rusqlite::params![pid],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
                ).ok();

                if let Some((title, activity_type, creator)) = details {
                    let evt_id = Uuid::new_v4().to_string();
                    let created_at = Utc::now().to_rfc3339();
                    let _ = conn.execute(
                        "INSERT INTO calendar_events
                         (id, peak_name, activity_type, planned_date, status, event_type, created_by, created_at)
                         VALUES (?1,?2,?3,?4,'open','plan',?5,?6)",
                        rusqlite::params![evt_id, title, activity_type, winning_date, creator, created_at],
                    );
                    let _ = conn.execute(
                        "UPDATE proposals SET status='scheduled', calendar_event_id=?1 WHERE id=?2",
                        rusqlite::params![evt_id, pid],
                    );
                }
            } else {
                // No votes → cancel
                let _ = conn.execute(
                    "UPDATE proposals SET status='cancelled' WHERE id=?1",
                    rusqlite::params![pid],
                );
            }
        }
    }

    // Return all proposals
    let mut stmt = conn.prepare(
        "SELECT p.id FROM proposals p ORDER BY
         CASE p.status WHEN 'voting' THEN 0 WHEN 'scheduled' THEN 1 ELSE 2 END,
         p.voting_closes_at ASC"
    ).unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .filter_map(|id| fetch_proposal(conn, &id, my_uid))
        .collect()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn list_proposals(
    State(store): State<Store>,
    current: CurrentUser,
) -> Result<Json<Vec<Proposal>>, StatusCode> {
    let uid = current.user_id.clone();
    let proposals = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        auto_resolve(&conn, &uid)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(proposals))
}

pub async fn get_proposal(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Proposal>, StatusCode> {
    let uid = current.user_id.clone();
    let p = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        fetch_proposal(&conn, &id, &uid)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(p))
}

pub async fn create_proposal(
    State(store): State<Store>,
    current: CurrentUser,
    Json(body): Json<CreateProposal>,
) -> Result<Json<Proposal>, StatusCode> {
    if body.title.trim().is_empty() { return Err(StatusCode::UNPROCESSABLE_ENTITY); }

    let activity_type = body.activity_type.as_deref()
        .filter(|s| VALID_ACT.contains(s))
        .unwrap_or("hike").to_string();

    let id           = Uuid::new_v4().to_string();
    let now          = Utc::now();
    let closes_at    = (now + Duration::days(2)).to_rfc3339();
    let now_str      = now.to_rfc3339();
    let uid          = current.user_id.clone();
    let id2          = id.clone();
    let uid2         = uid.clone();
    let store_ins    = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store_ins.lock().unwrap();
        conn.execute(
            "INSERT INTO proposals (id,title,description,activity_type,created_by,created_at,voting_closes_at,status)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'voting')",
            rusqlite::params![id2, body.title.trim(), body.description, activity_type, uid2, now_str, closes_at],
        ).map_err(|e| { tracing::error!("create proposal: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;

        // Insert initial date options
        for date in &body.dates {
            if date.trim().is_empty() { continue; }
            let oid = Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT INTO proposal_date_options (id,proposal_id,date,suggested_by,created_at)
                 VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![oid, &id2, date.trim(), &uid2, now_str],
            );
        }
        Ok::<(), StatusCode>(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let store2 = store.clone();
    let uid3   = uid.clone();
    let p = tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        fetch_proposal(&conn, &id, &uid3)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(p))
}

pub async fn add_date_option(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<AddDateOption>,
) -> Result<Json<Proposal>, StatusCode> {
    if body.date.trim().is_empty() { return Err(StatusCode::UNPROCESSABLE_ENTITY); }

    let oid    = Uuid::new_v4().to_string();
    let now    = Utc::now().to_rfc3339();
    let uid    = current.user_id.clone();
    let id2    = id.clone();
    let uid2   = uid.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        // Check proposal is still open
        let status: Option<String> = conn.query_row(
            "SELECT status FROM proposals WHERE id=?1",
            rusqlite::params![id2], |r: &rusqlite::Row| r.get(0),
        ).ok();
        match status.as_deref() {
            None          => return Err(StatusCode::NOT_FOUND),
            Some("voting") => {}
            _             => return Err(StatusCode::UNPROCESSABLE_ENTITY),
        }
        // No duplicate dates per proposal
        let exists: bool = conn.query_row(
            "SELECT 1 FROM proposal_date_options WHERE proposal_id=?1 AND date=?2",
            rusqlite::params![&id2, body.date.trim()], |_| Ok(true),
        ).unwrap_or(false);
        if exists { return Err(StatusCode::CONFLICT); }

        conn.execute(
            "INSERT INTO proposal_date_options (id,proposal_id,date,suggested_by,created_at)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![oid, &id2, body.date.trim(), &uid2, now],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let uid3 = uid.clone();
    let p = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        fetch_proposal(&conn, &id, &uid3)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(p))
}

pub async fn vote(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<VoteRequest>,
) -> Result<Json<Proposal>, StatusCode> {
    let now  = Utc::now().to_rfc3339();
    let uid  = current.user_id.clone();
    let id2  = id.clone();
    let uid2 = uid.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        // Verify proposal is still voting
        let status: Option<String> = conn.query_row(
            "SELECT status FROM proposals WHERE id=?1",
            rusqlite::params![id2], |r| r.get(0),
        ).ok();
        match status.as_deref() {
            None           => return Err(StatusCode::NOT_FOUND),
            Some("voting") => {}
            _              => return Err(StatusCode::UNPROCESSABLE_ENTITY),
        }
        // Verify option belongs to this proposal
        let opt_ok: bool = conn.query_row(
            "SELECT 1 FROM proposal_date_options WHERE id=?1 AND proposal_id=?2",
            rusqlite::params![&body.date_option_id, &id2], |_| Ok(true),
        ).unwrap_or(false);
        if !opt_ok { return Err(StatusCode::NOT_FOUND); }

        // Upsert vote (one vote per user per proposal, can change)
        conn.execute(
            "INSERT INTO proposal_votes (proposal_id,user_id,date_option_id,voted_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(proposal_id,user_id) DO UPDATE SET date_option_id=excluded.date_option_id, voted_at=excluded.voted_at",
            rusqlite::params![id2, uid2, &body.date_option_id, now],
        ).map_err(|e| { tracing::error!("vote: {}", e); StatusCode::INTERNAL_SERVER_ERROR })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let uid3 = uid.clone();
    let p = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        fetch_proposal(&conn, &id, &uid3)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(p))
}

pub async fn unvote(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Proposal>, StatusCode> {
    let uid    = current.user_id.clone();
    let id2    = id.clone();
    let uid2   = uid.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "DELETE FROM proposal_votes WHERE proposal_id=?1 AND user_id=?2",
            rusqlite::params![id2, uid2],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let uid3 = uid.clone();
    let p = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        fetch_proposal(&conn, &id, &uid3)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(p))
}

pub async fn delete_proposal(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let uid  = current.user_id.clone();
    let id2  = id.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let creator: Option<String> = conn.query_row(
            "SELECT created_by FROM proposals WHERE id=?1",
            rusqlite::params![id2], |r| r.get(0),
        ).ok();
        match creator {
            None => return Err(StatusCode::NOT_FOUND),
            Some(c) if c != uid && !current.is_admin() => return Err(StatusCode::FORBIDDEN),
            _ => {}
        }
        conn.execute("DELETE FROM proposals WHERE id=?1", rusqlite::params![id2])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}
