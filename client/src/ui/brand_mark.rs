use forgearr_shared::brand::MARK_PATH;
use leptos::prelude::*;

#[component]
pub fn BrandMark() -> impl IntoView {
    view! { <img class="brand-mark" src=MARK_PATH alt="" /> }
}
