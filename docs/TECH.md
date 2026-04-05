# Architektura techniczna - Thaler AI

<!-- ⚠️ CKM SEPARATION WARNING ⚠️ -->
<!-- TECH.md is for IMPLEMENTATION FOCUS (HOW) -->
<!-- DON'T include: Business rationale, project timeline, user requirements, success metrics -->
<!-- DO include: Architecture, code structure, APIs, procedures, known issues -->
<!-- For business context → see PRD.md -->
<!-- For timeline and planning → see PLAN.md -->

**Autor:** Jerzy Maczewski
**Stack:** Rust / axum / Vanilla JS / Ollama API

---

## Przegląd architektury systemu

### Architektura wysokopoziomowa
```
┌─────────────────────────────────────────────────────┐
│              Localhost Web App (Rust)                 │
│                                                       │
│  ┌──────────────────┐    ┌──────────────────────┐    │
│  │   Frontend (JS)   │◄──►│   Backend (Rust)      │    │
│  │                    │    │                        │    │
│  │  • index.html      │    │  • main.rs (axum srv) │    │
│  │  • main.js         │    │  • anonymizer.rs (NER)│    │
│  │  • style.css       │    │                        │    │
│  │                    │    │                        │    │
│  │  fetch() ────────►│    │  ◄── REST API          │    │
│  └──────────────────┘    └──────────┬───────────┘    │
│  (served by axum,                    │                │
│   opened in browser)                 │                │
└──────────────────────────────────────┼────────────────┘
                                       │ HTTP (reqwest)
                                       ▼
                              ┌─────────────────┐
                              │   Ollama API     │
                              │ (configurable)   │
                              │                   │
                              │  Bielik 11B (PL)  │
                              │  Gemma4 26B (MoE)  │
                              └─────────────────┘
```

### Główne komponenty:
- **Frontend (przeglądarka):** Vanilla HTML/JS/CSS — serwowany przez axum, otwierany w domyślnej przeglądarce
- **Backend (Rust/axum):** REST API na localhost — upload plików, anonimizacja, deanonimizacja, proxy Ollama
- **Silnik anonimizacji:** NER przez Ollama → deterministyczne tokeny → zamiana w tekście
- **Czytniki plików:** calamine (XLSX), zip + quick-xml (DOCX), std::fs (MD/TXT/CSV)

### Cykl życia aplikacji:
1. Binarka uruchamia serwer HTTP axum na porcie 3000 (auto-inkrementacja jeśli zajęty, do 3100)
2. Otwiera domyślną przeglądarkę (na WSL przeglądarka Windows przez `cmd.exe`, na Linuxie natywna)
3. Frontend wysyła heartbeat co 5s (`POST /api/heartbeat`), aby utrzymać serwer przy życiu
4. Serwer obsługuje odświeżanie strony bez zamykania (bez `beforeunload` kill)
5. Jeśli brak heartbeat przez 120s (przeglądarka zamknięta/zcrashowana) → serwer się wyłącza
6. `visibilitychange` event wysyła natychmiastowy heartbeat gdy użytkownik wraca na kartę (obejście throttlowania timerów w tle przez przeglądarki)

### Przepływ danych:
```
Plik (XLSX/DOCX/MD/TXT/CSV) → Browser (<input type="file">)
    → FormData upload → POST /api/load-file → Rust parses file
    → textarea input → POST /api/anonymize {text, source_file, categories?}
    → Rust: chunk text → Ollama NER (user-selected model)
    → extract entities → create tokens → replace in text
    → return AnonymizeResult {text, entities_found, model_used}
    → Frontend: output textarea + mapping table + GET /api/export-map
    → Download: browser download (.anon.md + .map.json)
    → [optional] POST /api/deanonymize {anon_text, map_json} → restore original
```

---

## Obecne możliwości

### Zaimplementowane funkcje:
- **Import XLSX:** calamine → format tabeli markdown — ✅ Gotowe
- **Import DOCX:** zip + quick-xml → ekstrakcja tekstu — ✅ Gotowe
- **Import MD/TXT/CSV:** odczyt w przeglądarce + backend fallback — ✅ Gotowe
- **NER przez Ollama:** model wybrany przez użytkownika, bez fallbacku — ✅ Gotowe
- **Panel logów:** logi w czasie rzeczywistym (odpowiedzi modelu, wykryte encje, błędy) — ✅ Gotowe
- **Parser JSON:** obsługa obciętych odpowiedzi LLM, bloków markdown, brakujących nawiasów, fallback regex — ✅ Gotowe
- **Filtry kategorii:** 7 checkboxów w UI (Osoby, Adresy, Firmy, Numery ID, Kwoty, Kontakt, Daty) z tooltipami — ✅ Gotowe
- **Wyświetlanie wersji:** wersja z Cargo.toml w nagłówku UI — ✅ Gotowe
- **Deterministyczne tokeny:** [TH_FIRMA_001], [TH_OSOBA_002] itp. (prefiks TH_ zapobiega kolizjom) — ✅ Gotowe
- **Eksport DOCX→DOCX:** anonimizacja/deanonimizacja z zachowaniem formatowania Word — ✅ Gotowe
- **Eksport XLSX→XLSX:** anonimizacja/deanonimizacja przez sharedStrings + komórki numeryczne — ✅ Gotowe
- **Statystyki deanonimizacji:** licznik zamienionych tokenów + raport brakujących — ✅ Gotowe
- **De-anonimizacja:** odwrócenie token→oryginał (tekst + DOCX) — ✅ Gotowe
- **Tabela mapowań w UI:** Token ↔ typ ↔ oryginał — ✅ Gotowe
- **Ciemny/jasny motyw UI:** przełącznik z autodetekcją systemową (`prefers-color-scheme`) — ✅ Gotowe
- **Konfiguracja Ollama w UI:** pole URL + lista modeli — ✅ Gotowe
- **Auto-shutdown:** heartbeat 120s + visibilitychange — ✅ Gotowe
- **Autodetekcja portu:** zakres 3000-3100 — ✅ Gotowe
- **Build .deb:** paczka via cargo-deb — ✅ Gotowe
- **Build .msi:** instalator Windows via cargo-wix (menu Start, ikona, auto-kill przy deinstalacji) — ✅ Gotowe
- **CI/CD:** GitHub Actions — auto-build .deb + .msi na push do dev i tag v* — ✅ Gotowe
- **Ikona aplikacji:** maski teatralne, osadzona w .exe (winres), .deb (pixmaps), .msi (WiX) — ✅ Gotowe
- **Windows:** ukryta konsola (`windows_subsystem`), auto-kill przy deinstalacji (taskkill) — ✅ Gotowe
- **Linux:** auto-kill przy deinstalacji (prerm killall) — ✅ Gotowe

---

## Znane ograniczenia

- XLSX → markdown traci formatowanie (scalone komórki, kolory) — calamine zwraca tylko dane tekstowe
- Brak podpisu cyfrowego — Windows SmartScreen wyswietla ostrzezenie przy pierwszym uruchomieniu

---

## Stack technologiczny

### Frontend:
- **Framework:** Vanilla HTML/JS (zero zależności)
- **Język:** ES6+ JavaScript
- **Styl:** Custom CSS, ciemny/jasny motyw z autodetekcją systemową
- **Pliki:** `<input type="file">` do uploadu, `Blob` + link do pobrania
- **Komunikacja z serwerem:** `fetch()` REST API

### Backend:
- **Język:** Rust (edition 2021)
- **Framework:** axum 0.8 (serwer HTTP)
- **Klient HTTP:** reqwest 0.12 (async, JSON, 5s connect timeout, 300s request timeout)
- **Czytnik XLSX:** calamine 0.26
- **Czytnik DOCX:** zip 2 + quick-xml 0.37
- **Zamiana tekstu:** aho-corasick 1 (single-pass multi-pattern replace)
- **Serializacja:** serde + serde_json
- **Runtime async:** tokio (full features)
- **Otwieranie przeglądarki:** open 5 (z fallbackiem `cmd.exe` na WSL)
- **Embedding frontendu:** `include_str!()` — HTML/JS/CSS kompilowane do binarki

### Usługi zewnętrzne:
- **Ollama API:** REST API — `/api/chat` (wykrywanie encji) i `/api/tags` (lista modeli)
- **Modele:** konfigurowalne w UI, brak domyślnego — użytkownik wybiera z listy
- **Przetestowane modele:** Bielik 11B Q8_0 (szybki, polski), Gemma4 26B A4B Q4_K_M (wolniejszy, dokładniejszy)

### Budowanie i dystrybucja:
- **Build:** `cargo build --release` (w `src-tauri/`)
- **Linux:** .deb via `cargo-deb` (metadane w `Cargo.toml [package.metadata.deb]`)
- **Windows:** .msi via `cargo-wix` (WiX template w `wix/main.wxs`, ikona via `winres`)
- **CI/CD:** GitHub Actions — auto-build na push do `dev` i tag `v*`

---

## Organizacja kodu

### Struktura katalogów:
```
thaler-ai/
├── docs/
│   ├── PRD.md              # Wymagania (CO i DLACZEGO)
│   ├── PLAN.md             # Plan implementacji (KIEDY)
│   └── TECH.md             # Architektura techniczna (JAK)
├── src/                     # Frontend (wbudowany w binarkę)
│   ├── index.html          # Główny layout UI
│   ├── main.js             # Wywołania API, obsługa zdarzeń
│   └── style.css           # Ciemny/jasny motyw, responsywny layout
├── src-tauri/               # Backend (Rust)
│   ├── Cargo.toml          # Zależności + metadane cargo-deb/cargo-wix
│   ├── build.rs            # Build script (winres — ikona w .exe)
│   ├── assets/             # .desktop file, prerm script
│   ├── icons/              # Ikony aplikacji (PNG, ICO)
│   ├── wix/                # WiX template (main.wxs, License.rtf)
│   └── src/
│       ├── main.rs         # Serwer axum, routing, handlery
│       └── anonymizer.rs   # Silnik wykrywania encji, mapowanie, tokeny
├── CLAUDE.md               # Instrukcje dla AI asystenta
├── README.md               # Dokumentacja użytkownika
├── LICENSE                  # Licencja MIT
└── .gitignore
```

### Konwencje:
- **Rust:** snake_case funkcje/zmienne, PascalCase struktury
- **JS:** camelCase funkcje/zmienne, prefiks $ dla elementów DOM
- **API:** endpointy REST pod `/api/*`, klucze JSON w snake_case

---

## Referencja API

### Endpointy REST (axum):

| Metoda | Ścieżka | Wejście | Wyjście | Opis |
|--------|---------|---------|---------|------|
| GET | `/` | — | HTML | Serwowanie frontendu |
| GET | `/main.js` | — | JS | Serwowanie JavaScript |
| GET | `/style.css` | — | CSS | Serwowanie stylów |
| GET | `/api/check-ollama` | — | `"OK"` | Test połączenia z Ollama |
| GET | `/api/list-models` | — | `["model1", ...]` | Lista modeli Ollama |
| GET | `/api/get-config` | — | `{url, model}` | Aktualna konfiguracja |
| POST | `/api/set-config` | `{url, model}` | `"ok"` | Zmiana URL/modelu Ollama |
| POST | `/api/load-file` | multipart file | `"text..."` | Parsowanie pliku do tekstu |
| POST | `/api/anonymize` | `{text, source_file, categories?}` | `AnonymizeResult` | Wykrywanie encji + tokenizacja |
| GET | `/api/get-mapping` | — | `[EntityInfo, ...]` | Tabela mapowań |
| GET | `/api/export-map` | — | `AnonMap JSON` | Pełna mapa do zapisu |
| GET | `/api/export-anon-native` | — | DOCX/XLSX bytes | Eksport zanonimizowanego pliku (format natywny) |
| POST | `/api/deanonymize` | `{anon_text, map_json}` | `"restored text"` | Odtworzenie oryginału (tekst) |
| POST | `/api/deanonymize-docx` | multipart: file + map_json | DOCX bytes | Odtworzenie oryginału (DOCX) |
| GET | `/api/logs` | — | `[String]` | Pobranie nowych logów (polling) |
| POST | `/api/heartbeat` | — | 200 OK | Podtrzymanie serwera |
| POST | `/api/shutdown` | — | `"bye"` | Zamknięcie serwera |

### Użycie Ollama API:

```
POST {ollama_url}/api/chat
{
  "model": "<selected model>",
  "messages": [
    {"role": "system", "content": "You are a document anonymization expert..."},
    {"role": "user", "content": "<NER prompt with document text>"}
  ],
  "stream": false,
  "options": {"temperature": 0.1, "num_predict": 4096}
}
```

Response parsed from `message.content` → JSON array of `{"text": "...", "type": "PERSON|COMPANY|..."}`.

### Mapowanie kategorii → typy NER:

| UI Checkbox | NER Types | Tooltip |
|-------------|-----------|---------|
| Osoby | PERSON | PERSON |
| Adresy | ADDRESS | ADDRESS |
| Firmy | COMPANY | COMPANY |
| Numery ID | NIP, REGON, KRS, PESEL, CONTRACT_ID, OTHER_ID | NIP, REGON, KRS, PESEL, CONTRACT_ID, OTHER_ID |
| Kwoty | AMOUNT, BANK_ACCOUNT | AMOUNT, BANK_ACCOUNT |
| Kontakt | PHONE, EMAIL | PHONE, EMAIL |
| Daty | DATE | DATE |

**Zasada:** Prompt zawsze zawiera wszystkie 14 typów encji (lepsza klasyfikacja). Kategorie kontrolują tylko które encje są tokenizowane w wyniku (filtr post-hoc).

### Typy encji:

| Typ | Polski token | Przykład |
|------|-------------|---------|
| PERSON | OSOBA | [TH_OSOBA_001] |
| COMPANY | FIRMA | [TH_FIRMA_002] |
| AMOUNT | KWOTA | [TH_KWOTA_003] |
| DATE | DATA | [TH_DATA_001] |
| ADDRESS | ADRES | [TH_ADRES_001] |
| PHONE | TELEFON | [TH_TELEFON_001] |
| EMAIL | EMAIL | [TH_EMAIL_001] |
| CONTRACT_ID | UMOWA | [TH_UMOWA_001] |
| NIP | NIP | [TH_NIP_001] |
| REGON | REGON | [TH_REGON_001] |
| KRS | KRS | [TH_KRS_001] |
| BANK_ACCOUNT | KONTO | [TH_KONTO_001] |
| PESEL | PESEL | [TH_PESEL_001] |
| OTHER_ID | ID | [TH_ID_001] |

---

## Algorytm wykrywania encji

### Pipeline (diagram):

```
┌─────────────────────────────────────────────────────────┐
│  1. PODZIAŁ TEKSTU                                       │
│     dokument → segmenty ~3000 znaków                    │
│     faza 1: \n\n (akapity) → faza 2: \n (wiersze)      │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  2. WYKRYWANIE (LLM)                          per chunk │
│     prompt NER → Ollama /api/chat → odpowiedź JSON      │
│     retry: 3 próby, exponential backoff (0s→2s→4s)      │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  3. PARSOWANIE JSON                    6-poziomowy fallback │
│     direct parse → extract [...] → repair truncated     │
│     → wrap bare objects → strip comma → regex fallback  │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  4. DEDUPLIKACJA                                         │
│     po tekście (case-insensitive), usunięcie pustych    │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  5. REGEX SAFETY NET                    deterministyczny │
│     wzorce pominiete przez model (nr dowodu itp.)       │
│     → patrz: Reguły deterministyczne                    │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  6. FILTR KATEGORII                          post-hoc   │
│     usunięcie encji z wyłączonych checkboxów UI         │
│     nieznane typy → przypisane do kategorii "Numery ID" │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  7. TOKENIZACJA                          dynamiczne typy │
│     encja → token [TH_TYP_NNN]                         │
│     znane typy: TH_OSOBA, TH_FIRMA, TH_NIP, ...        │
│     nieznane: model wymyśla typ → TH_NR_ARIMR itp.     │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│  8. ZAMIANA W TEKŚCIE                                    │
│     Aho-Corasick single-pass O(n), longest match first  │
└─────────────────────────────────────────────────────────┘
```

### Szczegóły kroków:

1. **Podział** dokumentu na segmenty ~3000 znaków (dwufazowy):
   - **Faza 1:** podział na `\n\n` (granice akapitów — dla dokumentów tekstowych)
   - **Faza 2:** jeśli segment > max_chars, podział na `\n` (granice wierszy — dla XLSX/CSV)
2. **Wywołanie modelu** z promptem NER → parsowanie odpowiedzi JSON
3. **Ekstrakcja JSON** z odpowiedzi (6-poziomowy fallback):
   1. Bezpośrednie parsowanie JSON array
   2. Wyciągnięcie `[...]` z bloków markdown
   3. Naprawa obciętego JSON (znalezienie ostatniego `}`, dodanie `]`)
   4. Owrapowanie obiektów bez tablicy w `[]` (usunięcie trailing comma)
   5. Usunięcie trailing comma wewnątrz `[...]`
   6. **Fallback regex** — wyciągnięcie pojedynczych obiektów `{"text":"...","type":"..."}` niezależnie od formatowania
4. **Deduplikacja** po tekście (case-insensitive)
5. **Regex safety net** — deterministyczne wykrywanie wzorców pominiętych przez model (patrz: [Reguły deterministyczne](#reguły-deterministyczne-regex-safety-net))
6. **Filtr kategorii post-hoc** — usunięcie encji z wyłączonych typów (na podstawie checkboxów). Nieznane typy (wymyślone przez model) przypisane do kategorii "Numery ID"
7. **Tworzenie tokenów** — deterministyczne, z dynamicznymi typami:
   - Znane typy: `PERSON` → `[TH_OSOBA_001]`, `COMPANY` → `[TH_FIRMA_001]` itp.
   - Nieznane typy: model wymyśla typ → token go odzwierciedla, np. `NR_ARIMR` → `[TH_NR_ARIMR_001]`
8. **Zamiana** w tekście — single-pass przez Aho-Corasick (O(n), najdłuższe dopasowania priorytetowe)

### Uwaga dot. DOCX:
- Bielik 11B jest wrażliwy na białe znaki — `\n\n` między akapitami obniża skuteczność z 7 do 1 encji
- Parser DOCX używa pojedynczego `\n` między akapitami (zachowuje `<w:br/>` i `<w:tab/>`)

---

## Reguły deterministyczne (regex safety net)

Moduł regex działa **po** wykrywaniu przez LLM i **po** deduplikacji — łapie wzorce, które mały model mógł pominąć. Każda reguła to para: wzorzec regex + typ NER. Encje znalezione przez regex nie duplikują się z tymi z LLM (sprawdzanie po `seen` set).

W logach wykrycia regex oznaczone są jako `regex fallback: '<wartość>' → <typ>`.

### Aktywne reguły:

| Wzorzec | Regex | Typ NER | Przykład | Uwagi |
|---------|-------|---------|----------|-------|
| Nr dowodu osobistego | `\b[A-Z]{3}\d{6}\b` | OTHER_ID | ABC123456 | 3 wielkie litery + 6 cyfr |

### Jak dodać nową regułę:

W `anonymizer.rs`, tablica `regex_patterns` w funkcji `anonymize()`:

```rust
let regex_patterns: &[(&str, &str)] = &[
    (r"\b[A-Z]{3}\d{6}\b", "OTHER_ID"),  // Polish ID card
    // Nowe reguły dodawać tutaj:
    // (r"WZORZEC", "TYP_NER"),
];
```

**Zasada:** Dodawaj reguły tylko dla wzorców o niskim ryzyku false positive (unikalne formaty, nie ogólne ciągi cyfr).

---

## Bezpieczeństwo

### Mechanizmy bezpieczeństwa:
- **Lokalność danych:** całe przetwarzanie przez lokalne Ollama — zero wywołań do chmury
- **Tylko localhost:** serwer nasłuchuje na `127.0.0.1` — niewidoczny w sieci
- **Mapowanie w RAM + jawny zapis:** mapowanie istnieje w pamięci podczas sesji; zapis na dysk tylko gdy użytkownik kliknie "Pobierz plik + mapę"
- **Brak telemetrii:** zero trackingu, zero analityki, zero phone-home
- **Auto-shutdown:** serwer kończy pracę gdy przeglądarka zamknięta (timeout heartbeat 120s, odporny na throttling tabów w tle) — brak osieroconych procesów
- **Timeout połączenia:** 5s connect, 300s request — zapobiega zawieszeniu przy niedostępnym Ollama
- **Polityka retry:** exponential backoff (3 próby na chunk) — przejściowe błędy Ollama obsługiwane w kodzie

---

## Budowanie i wdrożenie

### Rozwój:
```bash
cd src-tauri
cargo run --release      # Build + run server
# Browser opens automatically at http://localhost:3000
```

### Build produkcyjny:
```bash
cd src-tauri
cargo build --release    # Binary: target/release/thaler-ai
```

### Paczka .deb (cargo-deb):
```bash
cd src-tauri
cargo deb                # Build + package .deb
# Output: target/debian/thaler-ai_<version>_amd64.deb
sudo dpkg -i target/debian/thaler-ai_*.deb
thaler-ai                # Run — opens browser
```

### Instalator Windows (.msi):
```bash
cd src-tauri
cargo build --release
cargo wix --no-build     # Package .msi from WiX template
# Output: target/wix/thaler-ai-<version>-x86_64.msi
```

### CI/CD (GitHub Actions):
- **Trigger:** push do `dev` (pre-release) lub tag `v*` (stabilny release)
- **Linux job:** `cargo build --release` + `cargo deb --no-build` → .deb
- **Windows job:** `cargo build --release` + `cargo wix --no-build` → .msi
- **Artefakty:** GitHub Releases (`dev-latest` pre-release lub wersjonowany release)

### Wynik budowania:
```
src-tauri/target/debian/*.deb                # .deb package (~3 MB)
src-tauri/target/wix/*.msi                   # .msi installer (~6 MB)
```

---

*Ten dokument jest źródłem prawdy dla implementacji technicznej. Aktualizuj wraz z rozwojem systemu.*
