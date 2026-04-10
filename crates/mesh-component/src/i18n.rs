/// I18n block — per-locale translation strings embedded in a component.
use std::collections::HashMap;

/// Translations embedded in a component file.
///
/// Outer key = locale (`"en"`, `"fr"`), inner map = message key → translation.
///
/// ```text
/// <i18n>
/// [en]
/// greeting = "Hello"
/// farewell = "Goodbye"
///
/// [fr]
/// greeting = "Bonjour"
/// farewell = "Au revoir"
/// </i18n>
/// ```
#[derive(Debug, Clone)]
pub struct I18nBlock {
    pub entries: HashMap<String, HashMap<String, String>>,
}
