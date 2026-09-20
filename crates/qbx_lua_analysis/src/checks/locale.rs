use super::{FileInput, Sink};
use crate::locale::locale_usage;
use crate::rules;

pub(super) fn check(input: &FileInput, sink: &mut Sink) {
    let Some(locale) = input.locale else { return };
    for (key, span) in locale_usage(input.chunk).keys {
        if !locale.contains(&key) {
            let file = locale.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            sink.report(rules::UNKNOWN_LOCALE_KEY, span, format!("locale key '{key}' is not defined in locales/{file}"));
        }
    }
}
