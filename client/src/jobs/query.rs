use crate::route::encode_segment;
use crate::ui::filters::JobStateFilter;
use forgearr_shared::jobs::JobErrorCode;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct JobsQuery {
    pub state: Option<String>,
    pub library: Option<Uuid>,
    pub volume: Option<String>,
    pub error: Option<JobErrorCode>,
    pub job: Option<Uuid>,
    pub step: Option<i32>,
}

impl JobsQuery {
    pub fn current() -> Self {
        let search = leptos::prelude::window()
            .location()
            .search()
            .unwrap_or_default();
        let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
            return Self::default();
        };
        Self {
            state: params.get("state").filter(|state| {
                matches!(
                    state.as_str(),
                    "active" | "completed" | "succeeded" | "failed"
                )
            }),
            library: params.get("library").and_then(|id| id.parse().ok()),
            volume: params.get("volume"),
            error: params
                .get("error")
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok()),
            job: params.get("job").and_then(|id| id.parse().ok()),
            step: params.get("step").and_then(|step| step.parse().ok()),
        }
    }

    pub fn state_name(state: JobStateFilter) -> Option<&'static str> {
        match state {
            JobStateFilter::Any => None,
            JobStateFilter::Active => Some("active"),
            JobStateFilter::Completed => Some("completed"),
            JobStateFilter::Failed => Some("failed"),
        }
    }

    pub fn list_params(&self) -> String {
        [
            self.state.as_ref().map(|value| format!("state={value}")),
            self.library.map(|id| format!("library={id}")),
            self.volume
                .as_ref()
                .map(|value| format!("volume={}", encode_segment(value))),
            self.error.map(|error| format!("error={}", error.as_str())),
        ]
        .into_iter()
        .flatten()
        .map(|item| format!("&{item}"))
        .collect()
    }
}
