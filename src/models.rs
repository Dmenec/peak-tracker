use serde::{Deserialize, Serialize};

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

/// A calendar entry. Can be either a "plan" (future ascent) or an
/// "ascent" (a completed climb that counts toward the summit tally).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalendarEvent {
    pub id: String,
    pub peak_name: String,
    pub planned_date: String,
    pub companions: Option<String>,
    pub notes: Option<String>,
    pub status: String,
    /// "plan" or "ascent"
    pub event_type: String,
    pub duration_hours: Option<f64>,
    pub difficulty: Option<String>,
    pub created_at: String,
    /// GPS coordinates — only present when the event was created from the map
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEvent {
    pub peak_name: String,
    pub planned_date: String,
    pub companions: Option<String>,
    pub notes: Option<String>,
    /// Initial status — defaults to "planned"
    pub status: Option<String>,
    /// Event type — defaults to "plan"; use "ascent" to record a completed climb directly
    pub event_type: Option<String>,
    pub duration_hours: Option<f64>,
    pub difficulty: Option<String>,
    /// GPS coordinates — supplied when creating a plan from the map
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Used when changing an event's status.
/// When status = "completed", the event is automatically promoted to event_type = "ascent".
#[derive(Debug, Deserialize)]
pub struct UpdateEventStatus {
    pub status: String,
    /// Optional — fills in ascent details when marking as completed
    pub duration_hours: Option<f64>,
    pub difficulty: Option<String>,
}

/// Full update of all editable event fields.
/// All fields are required — send null to clear optional ones.
#[derive(Debug, Deserialize)]
pub struct UpdateEvent {
    pub peak_name: String,
    pub planned_date: String,
    pub companions: Option<String>,
    pub notes: Option<String>,
    pub duration_hours: Option<f64>,
    pub difficulty: Option<String>,
}
