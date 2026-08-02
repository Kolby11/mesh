//! System-wide locale management with per-module translation catalogs,
//! fallback chains, and runtime locale switching.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSet {
    pub locale: String,
    pub messages: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LocaleEngine {
    active_locale: String,
    fallback_chain: Vec<String>,
    translations: HashMap<String, HashMap<String, String>>,
    /// `module_id → locale → key → value`, checked before the global pool.
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

    pub fn load_translations(&mut self, set: TranslationSet) {
        self.translations
            .entry(set.locale)
            .or_default()
            .extend(set.messages);
    }

    /// Takes precedence over global catalogs in [`Self::translate_for_module`].
    /// Also merged into the global pool so `translate` and `t("key")` still work.
    pub fn load_module_translations(&mut self, module_id: &str, set: TranslationSet) {
        self.module_translations
            .entry(module_id.to_string())
            .or_default()
            .entry(set.locale.clone())
            .or_default()
            .extend(set.messages.clone());
        self.load_translations(set);
    }

    /// Walks the fallback chain.
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

    /// Module catalog first, then global. Use for manifest text so module
    /// entries win and cross-module key collisions cannot bite.
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

    /// Owned counterpart to [`Self::translate_for_module`], for consumers such
    /// as the Luau runtime that retain a lookup table. Lower-priority locales
    /// are applied first so precedence matches individual lookups.
    pub fn effective_translations_for_module(&self, module_id: &str) -> HashMap<String, String> {
        let mut messages = HashMap::new();
        for locale in self.fallback_chain.iter().rev() {
            if let Some(catalog) = self.translations.get(locale) {
                messages.extend(catalog.clone());
            }
        }
        if let Some(module_locales) = self.module_translations.get(module_id) {
            for locale in self.fallback_chain.iter().rev() {
                if let Some(catalog) = module_locales.get(locale) {
                    messages.extend(catalog.clone());
                }
            }
        }
        messages
    }

    /// Interpolates `{name}` placeholders in one walk, so cost is
    /// O(template_len) regardless of how many args are supplied.
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

        assert_eq!(engine.translate("ok"), Some("OK"));
    }

    #[test]
    fn effective_module_catalog_matches_scoped_lookup_precedence() {
        let mut engine = LocaleEngine::with_fallback_locale("sk", "en");
        engine.load_translations(TranslationSet {
            locale: "sk".to_string(),
            messages: HashMap::from([
                ("shared".to_string(), "global-sk".to_string()),
                ("global".to_string(), "iba-global".to_string()),
            ]),
        });
        engine.load_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([("shared".to_string(), "global-en".to_string())]),
        });
        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".to_string(),
                messages: HashMap::from([("shared".to_string(), "module-en".to_string())]),
            },
        );

        let catalog = engine.effective_translations_for_module("@mesh/example");
        assert_eq!(catalog.get("shared"), Some(&"module-en".to_string()));
        assert_eq!(catalog.get("global"), Some(&"iba-global".to_string()));
    }
}
