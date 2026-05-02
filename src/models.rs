use serde::{Deserialize, Serialize};

// ── Peaks ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Peak {
    pub id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i32,
    pub ascent_date: Option<String>,
    pub notes: Option<String>,
    pub photo_url: Option<String>,
    pub difficulty: Option<String>,
    pub duration_hours: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePeak {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i32,
    pub ascent_date: Option<String>,
    pub notes: Option<String>,
    pub difficulty: Option<String>,
    pub duration_hours: Option<f64>,
}

// ── Users ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String, // "admin" | "user"
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePassword {
    pub current_password: Option<String>, // required for self; optional for admin
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfile {
    pub display_name: String,
}

// ── Calendar ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventParticipant {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub status: String, // "going" | "maybe" | "not_going"
    pub joined_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalendarEvent {
    pub id: String,
    pub peak_name: String,
    pub category: String,      // peak | plan
    pub activity_type: String, // peak: hike|via_ferrata|ski|trail_run|cycling|camping  plan: food|festival|culture|beach|social|travel|sport|accommodation|other
    pub planned_date: String,
    pub end_date: Option<String>,
    pub notes: Option<String>,
    pub difficulty: Option<String>,
    pub duration_hours: Option<f64>,
    pub max_participants: Option<i32>,
    pub cost_per_person: Option<f64>,
    pub currency: String,
    pub meeting_point: Option<String>,
    pub status: String,      // open | full | cancelled | completed
    pub event_type: String,  // plan | ascent (kept for compat)
    pub created_by: String,
    pub created_by_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub participant_count: i64,
    pub attendees: Vec<EventParticipant>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateEvent {
    pub peak_name: String,
    pub activity_type: Option<String>,
    pub planned_date: String,
    pub end_date: Option<String>,
    pub notes: Option<String>,
    pub difficulty: Option<String>,
    pub duration_hours: Option<f64>,
    pub max_participants: Option<i32>,
    pub cost_per_person: Option<f64>,
    pub currency: Option<String>,
    pub meeting_point: Option<String>,
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub category: Option<String>, // "peak" | "plan"
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEvent {
    pub peak_name: String,
    pub category: Option<String>,
    pub activity_type: Option<String>,
    pub planned_date: String,
    pub end_date: Option<String>,
    pub notes: Option<String>,
    pub difficulty: Option<String>,
    pub duration_hours: Option<f64>,
    pub max_participants: Option<i32>,
    pub cost_per_person: Option<f64>,
    pub currency: Option<String>,
    pub meeting_point: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventStatus {
    pub status: String,
    pub duration_hours: Option<f64>,
    pub difficulty: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RsvpRequest {
    pub status: String, // "going" | "maybe" | "not_going"
}

// ── Proposals ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProposalDateOption {
    pub id:           String,
    pub date:         String,
    pub suggested_by: String,
    pub suggested_by_name: String,
    pub vote_count:   i64,
    pub i_voted:      bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Proposal {
    pub id:               String,
    pub title:            String,
    pub description:      Option<String>,
    pub activity_type:    String,
    pub created_by:       String,
    pub created_by_name:  String,
    pub created_at:       String,
    pub voting_closes_at: String,
    pub status:           String, // voting | scheduled | cancelled
    pub calendar_event_id: Option<String>,
    pub date_options:     Vec<ProposalDateOption>,
    pub total_votes:      i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateProposal {
    pub title:         String,
    pub description:   Option<String>,
    pub activity_type: Option<String>,
    pub dates:         Vec<String>, // initial date suggestions
}

#[derive(Debug, Deserialize)]
pub struct AddDateOption {
    pub date: String,
}

#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    pub date_option_id: String,
}
