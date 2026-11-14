//! Styles de l'écran d'accès (installation et connexion).

pub const AUTH_CSS: &str = r#"
.auth-gate {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  padding: var(--space-3);
  background-color: var(--bg-app);
  overflow-y: auto;
}

.auth-gate[hidden] {
  display: none;
}

.auth-gate__card {
  width: min(100%, 26rem);
  padding: var(--space-4);
  background-color: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}

.auth-gate__brand {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: var(--space-3);
}

.auth-gate__logo {
  display: inline-grid;
  place-items: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: var(--radius-md);
  background-color: var(--primary);
  color: var(--primary-contrast);
  font-size: 0.75rem;
  font-weight: 800;
}

.auth-gate__name {
  font-weight: 700;
  color: var(--text-main);
}

.auth-gate__title {
  font-size: 1.375rem;
  font-weight: 700;
  color: var(--text-main);
}

.auth-gate__subtitle {
  margin: 0.25rem 0 var(--space-3);
  color: var(--text-dim);
}

.auth-gate__form {
  display: flex;
  flex-direction: column;
}

.auth-gate__form[hidden] {
  display: none;
}

.auth-gate__notice {
  margin: 0 0 var(--space-2);
  padding: 0.65rem 0.85rem;
  border-radius: var(--radius-sm);
  background-color: var(--warning-bg);
  color: var(--warning);
  font-size: 0.8125rem;
  line-height: 1.45;
}

.auth-gate__submit {
  width: 100%;
  justify-content: center;
}

.form-error {
  min-height: 1.25rem;
  margin: var(--space-2) 0 0;
  color: var(--danger-text);
  font-size: 0.8125rem;
}
"#;
