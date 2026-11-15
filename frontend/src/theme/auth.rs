//! Styles de l'écran d'accès (installation et connexion).

pub const AUTH_CSS: &str = r#"
.auth-gate {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  padding: var(--space-3);
  background:
    radial-gradient(circle at 50% 0%, var(--primary-bg), transparent 55%),
    var(--bg-app);
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
  box-shadow: var(--shadow-lg);
}

.auth-gate__card--wide {
  width: min(100%, 42rem);
}

.auth-role-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr));
  gap: 0.9rem;
}

.auth-role-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.5rem;
  padding: 1rem 1.05rem;
  text-align: left;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-main);
  cursor: pointer;
  transition: border-color 0.16s ease, box-shadow 0.16s ease, transform 0.16s ease;
}

.auth-role-card:hover {
  border-color: var(--primary);
  box-shadow: var(--shadow-glow);
  transform: translateY(-1px);
}

.auth-role-card:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.auth-role-card__top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  gap: 0.5rem;
}

.auth-role-card__icon {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-sm);
  background: var(--primary-bg);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.auth-role-card__badge {
  font-size: 0.6875rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-dim);
}

.auth-role-card__title {
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-main);
}

.auth-role-card__desc {
  font-size: 0.8125rem;
  line-height: 1.45;
  color: var(--text-dim);
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
