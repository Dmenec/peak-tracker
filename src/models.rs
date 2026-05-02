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
    pub activity_type: String, // hike | via_ferrata | ski | trail_run | cycling | camping | other
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
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEvent {
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
