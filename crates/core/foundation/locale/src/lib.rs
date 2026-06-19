/// Localization engine for MESH.
///
/// Provides system-wide locale management with per-module translation support,
/// fallback chains, and runtime locale switching.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A set of translations for a single locale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSet {
    pub locale: String,
    pub messages: HashMap<String, String>,
}

/// The locale engine manages the active locale and translation lookup.
#[derive(Debug, Clone)]
pub struct LocaleEngine {
    active_locale: String,
    fallback_chain: Vec<String>,
    translations: HashMap<String, HashMap<String, String>>,
    /// Per-module catalogs. Scoped lookup checks these before the global pool.
    /// Key: module_id → locale → translation key → value.
    module_translations: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

impl LocaleEngine {
    pub fn new(default_locale: impl Into<String>) -> Self {
        let locale = default_locale.into();
        Self {
            active_locale: locale.clone(),
            fallback_chain: vec![locale, "en".to_string()],
            translations: HashMap::new(),
            module_translations: HashMap::new(),
        }
    }

    pub fn with_fallback_locale(
        default_locale: impl Into<String>,
        fallback_locale: impl Into<String>,
    ) -> Self {
        let locale = default_locale.into();
        let fallback = fallback_locale.into();
        let mut fallback_chain = vec![locale.clone()];
        if fallback != locale {
            fallback_chain.push(fallback);
        }

        Self {
            active_locale: locale,
            fallback_chain,
            translations: HashMap::new(),
            module_translations: HashMap::new(),
        }
    }

    pub fn current(&self) -> &str {
        &self.active_locale
    }

    pub fn set_locale(&mut self, locale: impl Into<String>) {
        let locale = locale.into();
        self.fallback_chain.insert(0, locale.clone());
        self.fallback_chain.dedup();
        self.active_locale = locale;
    }

    /// Register translations for a locale.
    pub fn load_translations(&mut self, set: TranslationSet) {
        self.translations
            .entry(set.locale)
            .or_default()
            .extend(set.messages);
    }

    /// Register translations scoped to a specific module.
    ///
    /// Module-scoped translations take precedence over global ones in
    /// `translate_for_module`. All catalogs are also merged into the global pool
    /// so that `translate` and template `t("key")` calls continue to work.
    pub fn load_module_translations(&mut self, module_id: &str, set: TranslationSet) {
        self.module_translations
            .entry(module_id.to_string())
            .or_default()
            .entry(set.locale.clone())
            .or_default()
            .extend(set.messages.clone());
        self.load_translations(set);
    }

    /// Look up a translation key, walking the fallback chain.
    pub fn translate(&self, key: &str) -> Option<&str> {
        for locale in &self.fallback_chain {
            if let Some(messages) = self.translations.get(locale) {
                if let Some(value) = messages.get(key) {
                    return Some(value.as_str());
                }
            }
        }
        None
    }

    /// Look up a translation key scoped to a module, then fall back to global.
    ///
    /// Use this when resolving manifest text (keybind labels, layout labels,
    /// provider labels, resource labels) so module-specific catalog entries take
    /// precedence and accidental cross-module key collisions are avoided.
    pub fn translate_for_module<'a>(&'a self, key: &str, module_id: &str) -> Option<&'a str> {
        if let Some(module_locales) = self.module_translations.get(module_id) {
            for locale in &self.fallback_chain {
                if let Some(messages) = module_locales.get(locale) {
                    if let Some(value) = messages.get(key) {
                        return Some(value.as_str());
                    }
                }
            }
        }
        self.translate(key)
    }

    /// Translate with interpolation. Placeholders use `{name}` syntax.
    ///
    /// Walks the template once instead of doing `String::replace` per
    /// placeholder, so cost is O(template_len) regardless of how many
    /// args are supplied.
    pub fn translate_with(&self, key: &str, args: &HashMap<String, String>) -> Option<String> {
        let template = self.translate(key)?;
        let mut result = String::with_capacity(template.len());
        let mut remaining = template;
        while let Some(open) = remaining.find('{') {
            result.push_str(&remaining[..open]);
            let after_open = &remaining[open + 1..];
            if let Some(close) = after_open.find('}') {
                let name = &after_open[..close];
                match args.get(name) {
                    Some(value) => result.push_str(value),
                    None => {
                        // Unknown placeholder: preserve the literal `{name}`.
                        result.push('{');
                        result.push_str(name);
                        result.push('}');
                    }
                }
                remaining = &after_open[close + 1..];
            } else {
                // Unmatched `{` — emit the rest literally and stop scanning.
                result.push_str(&remaining[open..]);
                return Some(result);
            }
        }
        result.push_str(remaining);
        Some(result)
    }

    pub fn fallback_chain(&self) -> &[String] {
        &self.fallback_chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_translation() {
        let mut engine = LocaleEngine::new("en");
        engine.load_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([
                ("greeting".to_string(), "Hello, {name}!".to_string()),
                ("bye".to_string(), "Goodbye".to_string()),
            ]),
        });

        assert_eq!(engine.translate("bye"), Some("Goodbye"));

        let args = HashMap::from([("name".to_string(), "World".to_string())]);
        assert_eq!(
            engine.translate_with("greeting", &args),
            Some("Hello, World!".to_string())
        );
    }

    #[test]
    fn fallback_chain() {
        let mut engine = LocaleEngine::new("fr");
        engine.load_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([("ok".to_string(), "OK".to_string())]),
        });

        // "ok" is not in "fr", falls back to "en"
        assert_eq!(engine.translate("ok"), Some("OK"));
    }
}
