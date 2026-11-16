---
title: office-i18n
parent: Libraries
nav_order: 5
---

Translation catalogues in Fluent files, found by an id made from the English text. Version 0.1.0.

## Public items

| Item | Meaning |
|---|---|
| `Catalogs::new(files)` | catalogues from `(locale, text)` pairs; `codes()`, `locale_of(code)`, `message(language, id)` |
| `Lang::new(files)` | a language in use over catalogues: `language()`, `reset()`, `set(code)`, `codes()`, `text(id)`, `tr(english)` |
| `id_for(english)` | the id of an English label |
| `ids(file)` | the ids a catalogue file defines |

A label is written in English in the code and looked up with `tr`; the id is made from the text, so a label with no entry in a language is shown in English. Each app lists its catalogues in `crates/<app>-app/src/i18n.rs` and keeps the files in `crates/<app>-app/i18n/`.
