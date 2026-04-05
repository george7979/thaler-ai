# Plan implementacji - Thaler AI

<!-- ⚠️ CKM SEPARATION WARNING ⚠️ -->
<!-- PLAN.md is for TIMELINE FOCUS (WHEN) -->
<!-- DON'T include: Architecture details, code examples, business justifications -->
<!-- DO include: Phases, milestones, dependencies, task breakdown, priorities -->
<!-- For business requirements → see PRD.md -->
<!-- For technical implementation → see TECH.md -->

**Autor:** Jerzy Maczewski
**Start projektu:** 2026-03-26
**Aktualna wersja:** v0.4.2
**Faza:** Early access — publiczny dostęp, aktywny rozwój

---

## Co dalej

### Krytyczne:
- [ ] Testy na prawdziwych dokumentach — walidacja skuteczności wykrywania
- [x] GitHub Actions CI/CD — auto-build .deb (Linux) + .msi (Windows)

### Ważne:
- [ ] Edycja mapowań w UI — wykluczenie/dodanie encji ręcznie
- [ ] Losowe kwoty zamiast tokenów w XLSX — zachowanie formuł i formatowania (analiza: `docs/ANALIZA-KWOTY-XLSX.md`)
- [ ] Drag & drop — pliki do okna aplikacji
- [ ] Kopiowanie do schowka — zanonimizowany tekst
- [x] Profesjonalne ikony — maski teatralne (komedia/tragedia)

### Opcjonalne:
- [ ] Przetwarzanie wsadowe — folder z dokumentami
- [ ] Integracja MCP z Claude Code
- [ ] Profile anonimizacji per typ dokumentu
- [ ] Import PDF (tekst)

---

## Zależności

- **Ollama** — wymagany do wykrywania danych wrażliwych, musi być dostępny lokalnie lub w sieci
- **Min. 1 model LLM** na Ollama (Bielik 11B, Gemma4 26B lub inny)
- **CI/CD → Windows build** — potrzebny GitHub Actions runner z Windows

---

## Ryzyka

- **Jakość wykrywania na polskich dokumentach** — pominięte dane = potencjalny wyciek. Mitygacja: wybór modelu przez użytkownika, ręczna weryfikacja tabeli mapowań, filtry kategorii
- **Brak podpisu cyfrowego** — Windows SmartScreen wyświetla ostrzeżenie. Mitygacja: info w README

---

## Ukończone

### v0.4.2 (2026-04-05):
- ✅ Regex safety net — deterministyczne wykrywanie nr dowodu osobistego (`[A-Z]{3}\d{6}`)
- ✅ Dynamiczne typy encji — model wymyśla typ → token go odzwierciedla (np. `[TH_NR_ARIMR_001]`)
- ✅ Wzbogacony prompt NER (przykłady formatów nr dowodu, ARiMR)
- ✅ Nieznane typy przypisane do kategorii "Numery ID"

### v0.4.1 (2026-04-05):
- ✅ Ciemny/jasny motyw UI z autodetekcją systemową (`prefers-color-scheme`)
- ✅ Heartbeat odporny na throttling tabów w tle (timeout 30s → 120s + `visibilitychange`)
- ✅ Anonimizacja hiperlinków w XLSX (pliki `_rels/*.rels`)
- ✅ Fix path traversal w upload plików
- ✅ Aho-Corasick single-pass replacement (O(n) zamiast O(n²))
- ✅ GitHub Actions CI/CD — auto-build .deb + .msi
- ✅ Ikona aplikacji — maski teatralne (komedia/tragedia)
- ✅ Windows: ukryta konsola, ikona osadzona w .exe, auto-kill przy deinstalacji
- ✅ Linux: auto-kill przy deinstalacji (prerm script)

### v0.4.0 (2026-04-05):
- ✅ Fix parsera JSON — regex fallback dla zniekształconych odpowiedzi LLM
- ✅ Zmiana /api/generate → /api/chat — naprawa pustych odpowiedzi Gemma4
- ✅ Wyświetlanie wersji w UI z Cargo.toml
- ✅ Pełne logowanie odpowiedzi (bez limitu 300 znaków)
- ✅ Checkboxy kategorii — 7 przełączników z tooltipami typów
- ✅ Dynamiczny prompt — zawsze wysyła wszystkie typy, filtr post-hoc
- ✅ Domyślny URL Ollama → localhost:11434
- ✅ Typ PESEL jako osobna kategoria

### v0.3.0 (2026-03-27):
- ✅ Eksport DOCX→DOCX i XLSX→XLSX z zachowaniem formatowania
- ✅ Prefiks TH_ w tokenach (zapobiega kolizjom z tekstem dokumentu)
- ✅ De-anonimizacja z statystykami (X/Y tokenów zamieniono)
- ✅ Panel logów w UI
- ✅ Kopiowanie logów
- ✅ Plik .desktop w .deb
- ✅ Serwer odporny na odświeżanie przeglądarki

### v0.1–v0.2 (2026-03-26):
- ✅ Prototyp Python → backend Rust/axum
- ✅ Import DOCX, XLSX, MD/TXT/CSV
- ✅ Anonimizacja i de-anonimizacja
- ✅ Konfiguracja Ollama URL + wybór modelu w UI
- ✅ Heartbeat + auto-shutdown
- ✅ Autodetekcja portu (3000-3100)
- ✅ Parser JSON (obcięte odpowiedzi, bloki markdown)
- ✅ Build .deb

---

## Historia zmian

| **Data** | **Wersja** | **Zmiana** |
|----------|------------|------------|
| 2026-03-26 | v0.1–v0.2 | MVP + migracja Tauri → axum |
| 2026-03-27 | v0.3.0 | Eksport natywny DOCX/XLSX, tokeny TH_, statystyki deanonimizacji |
| 2026-04-05 | v0.4.0 | Filtry kategorii, /api/chat, fix parsera JSON, wyświetlanie wersji |
| 2026-04-05 | v0.4.1 | Motyw ciemny/jasny, fix heartbeat, anonimizacja hiperlinków XLSX, fix bezpieczeństwa |
| 2026-04-05 | v0.4.2 | Regex safety net, dynamiczne typy encji, CI/CD, ikona, Windows .msi |

---

*Ten plan jest dokumentem żywym. Aktualizuj regularnie na podstawie postępu.*
