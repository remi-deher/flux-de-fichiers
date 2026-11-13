//! Attente non bloquante dans le navigateur (`setTimeout`).

/// Attend `millis` millisecondes sans bloquer la page.
pub async fn sleep(millis: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window().map(|window| {
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)
        });
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
