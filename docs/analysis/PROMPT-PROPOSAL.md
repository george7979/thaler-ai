# Propozycja nowego promptu NER — v2

**Data:** 2026-04-09
**Status:** propozycja do wdrożenia
**Cel:** poprawa wykrywania dat, kwot słownych i encji pomijanych przez małe modele

## Kontekst

Obecny prompt (~400 tokenów) ma zbyt ogólne opisy typów encji. Małe modele (Bielik 11B, Gemma4) pomijają:
- kwoty zapisane słownie ("dwieście pięćdziesiąt tysięcy złotych")
- daty w polskim formatowaniu ("dnia 15 marca 2026 r.")
- kwoty z kontekstem "słownie:" traktowane jako dwa osobne obiekty
- numery umów/postępowań ukryte w treści

Nowy prompt (~1200 tokenów) dodaje rozbudowane opisy z podwariantami, sekcję "najczęściej pomijane" i pełny few-shot example. Przy kontekście Bielik 11B v3 = 32k tokenów i chunkach 3000 znaków — mieści się z dużym zapasem (~10% kontekstu).

## System message

```
You are a document anonymization expert specialized in Polish legal and business documents. You find ALL sensitive entities and return them as a JSON array. Respond ONLY with valid JSON, no markdown, no commentary. Pay special attention to amounts written as words and dates in Polish format.
```

## User prompt (szablon)

```
Jesteś ekspertem od anonimizacji dokumentów urzędowych i umów. Twoim zadaniem jest znalezienie WSZYSTKICH danych wrażliwych w tekście.

Zwróć TYLKO JSON array z obiektami, bez żadnego dodatkowego tekstu, komentarzy ani markdown.
Każdy obiekt musi mieć pola: "text" (dokładny tekst z dokumentu), "type" (typ encji).

## Typy encji:

- PERSON — imiona i nazwiska osób (np. "Jan Kowalski", "Anna Nowak-Wiśniewska")
- COMPANY — nazwy firm, spółek, instytucji (np. "ABC Sp. z o.o.", "Urząd Miasta Krakowa")
- AMOUNT — kwoty pieniężne w KAŻDEJ formie:
  • liczbowe: "8 934 000,00 PLN", "1.5 mln zł", "123,45 EUR", "50 000,00 zł"
  • słowne: "pięćdziesiąt tysięcy złotych", "dwa miliony trzysta tysięcy 00/100"
  • mieszane: "50 000,00 zł (słownie: pięćdziesiąt tysięcy złotych 00/100)" — traktuj jako JEDEN obiekt z pełnym tekstem łącznie z częścią "słownie:"
  • procenty od kwot: "2% wartości umowy" NIE jest kwotą — ignoruj
- DATE — konkretne daty w KAŻDYM formacie:
  • "15 marca 2026 r.", "15.03.2026", "2026-03-15"
  • "dnia 15 marca 2026 roku", "z dnia 10.01.2026 r."
  • "do dnia 31.12.2026", "w terminie do 30 czerwca 2026 r."
  • "od 01.01.2026 do 31.12.2026" — to DWA osobne obiekty DATE
  • NIE oznaczaj ogólników: "w ciągu 14 dni", "30 dni roboczych" to NIE są daty
- ADDRESS — pełne adresy (np. "ul. Kwiatowa 15, 00-001 Warszawa")
- PHONE — numery telefonów (np. "+48 123 456 789", "123-456-789")
- EMAIL — adresy email
- CONTRACT_ID — numery umów, postępowań, sygnatur (np. "ZP/385/LZA/2025", "DZP.26.1.2025")
- NIP — numery NIP (np. "NIP: 123-456-78-90", "NIP 1234567890")
- REGON — numery REGON
- KRS — numery KRS
- BANK_ACCOUNT — numery kont bankowych (np. "PL 12 3456 7890 1234 5678 9012 3456")
- PESEL — numery PESEL (11 cyfr)
- OTHER_ID — inne identyfikatory (nr dowodu, nr ewidencyjny, nr ARiMR)

## Zasady:

1. Znajdź KAŻDE wystąpienie — nawet jeśli ta sama encja pojawia się wielokrotnie, wypisz ją RAZ
2. Zachowaj DOKŁADNY tekst z dokumentu (wielkość liter, spacje, interpunkcja)
3. Nie łącz różnych encji — "Jan Kowalski z Firma Sp. z o.o." to DWA obiekty
4. NIE anonimizuj nazw ogólnych: "Zamawiający", "Wykonawca", "Strona", "Polska"
5. NIE anonimizuj terminów prawnych, technicznych, nazw produktów, standardów
6. Kwota z częścią słowną w nawiasie to JEDEN obiekt — nie dziel na dwa

## Najczęściej pomijane — zwróć szczególną uwagę:

- Kwoty zapisane SŁOWNIE (np. "dwieście pięćdziesiąt tysięcy złotych")
- Daty z polskim formatowaniem (np. "dnia 15 marca 2026 r.")
- Numery umów/postępowań ukryte w treści (np. "na podstawie umowy ZP/123/2025")
- Numery kont bankowych (26 cyfr, czasem z PL na początku)

## Przykład:

Tekst: "Firma XYZ Sp. z o.o. (NIP: 987-654-32-10) z siedzibą przy ul. Leśnej 5, 30-001 Kraków zobowiązuje się zapłacić kwotę 250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100) na konto nr PL 61 1090 1014 0000 0712 1981 2874 w terminie do dnia 30 czerwca 2026 r. Osoba do kontaktu: Maria Wiśniewska, tel. 512 345 678."

Odpowiedź:
[
  {"text": "XYZ Sp. z o.o.", "type": "COMPANY"},
  {"text": "987-654-32-10", "type": "NIP"},
  {"text": "ul. Leśnej 5, 30-001 Kraków", "type": "ADDRESS"},
  {"text": "250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100)", "type": "AMOUNT"},
  {"text": "PL 61 1090 1014 0000 0712 1981 2874", "type": "BANK_ACCOUNT"},
  {"text": "30 czerwca 2026 r.", "type": "DATE"},
  {"text": "Maria Wiśniewska", "type": "PERSON"},
  {"text": "512 345 678", "type": "PHONE"}
]

## Tekst do analizy:
---
{tekst dokumentu}
---

Odpowiedz TYLKO validnym JSON array:
```

## Parametry Ollama

```json
{
  "temperature": 0.1,
  "num_predict": 4096
}
```

## Zmiany vs obecny prompt

| Aspekt | v1 (obecny) | v2 (propozycja) |
|--------|-------------|------------------|
| Rozmiar | ~400 tokenów | ~1200 tokenów |
| Opisy typów | jednolinijkowe | rozbudowane z podwariantami |
| Kwoty słowne | brak | jawnie wymienione z przykładami |
| Daty | 2 formaty | 6+ formatów + co NIE jest datą |
| Few-shot | brak | pełny przykład z 8 encjami |
| Sekcja "pomijane" | brak | dedykowana sekcja |
| Negatywne przykłady | minimalne | procenty, ogólniki czasowe |

## Externalizacja promptu — koncepcja

### Problem

Nie można dać użytkownikowi pliku z placeholderami `{{ENTITY_TYPES}}` i `{{DOCUMENT_TEXT}}` — ryzyko przypadkowego usunięcia, niejasna struktura.

### Rozwiązanie: plik `.md` z sekcjami parsowanymi po nagłówkach

Użytkownik edytuje plik `prompt.md` obok binarki. Zawiera **tylko edytowalne sekcje** — bez typów encji i tekstu dokumentu (te wstawia kod).

#### Przykładowy `prompt.md`:

```markdown
# System Message
You are a document anonymization expert specialized in Polish legal and business documents. You find ALL sensitive entities and return them as a JSON array. Respond ONLY with valid JSON, no markdown, no commentary. Pay special attention to amounts written as words and dates in Polish format.

# Zasady
1. Znajdź KAŻDE wystąpienie — nawet jeśli ta sama encja pojawia się wielokrotnie, wypisz ją RAZ
2. Zachowaj DOKŁADNY tekst z dokumentu (wielkość liter, spacje, interpunkcja)
3. Nie łącz różnych encji — "Jan Kowalski z Firma Sp. z o.o." to DWA obiekty
4. NIE anonimizuj nazw ogólnych: "Zamawiający", "Wykonawca", "Strona", "Polska"
5. NIE anonimizuj terminów prawnych, technicznych, nazw produktów, standardów
6. Kwota z częścią słowną w nawiasie to JEDEN obiekt — nie dziel na dwa

# Najczęściej pomijane
- Kwoty zapisane SŁOWNIE (np. "dwieście pięćdziesiąt tysięcy złotych")
- Daty z polskim formatowaniem (np. "dnia 15 marca 2026 r.")
- Numery umów/postępowań ukryte w treści (np. "na podstawie umowy ZP/123/2025")
- Numery kont bankowych (26 cyfr, czasem z PL na początku)

# Przykład
Tekst: "Firma XYZ Sp. z o.o. (NIP: 987-654-32-10) z siedzibą przy ul. Leśnej 5, 30-001 Kraków zobowiązuje się zapłacić kwotę 250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100) na konto nr PL 61 1090 1014 0000 0712 1981 2874 w terminie do dnia 30 czerwca 2026 r. Osoba do kontaktu: Maria Wiśniewska, tel. 512 345 678."

Odpowiedź:
[
  {"text": "XYZ Sp. z o.o.", "type": "COMPANY"},
  {"text": "987-654-32-10", "type": "NIP"},
  {"text": "ul. Leśnej 5, 30-001 Kraków", "type": "ADDRESS"},
  {"text": "250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100)", "type": "AMOUNT"},
  {"text": "PL 61 1090 1014 0000 0712 1981 2874", "type": "BANK_ACCOUNT"},
  {"text": "30 czerwca 2026 r.", "type": "DATE"},
  {"text": "Maria Wiśniewska", "type": "PERSON"},
  {"text": "512 345 678", "type": "PHONE"}
]
```

#### Jak kod Rust to skleja:

```
1. Parsuj prompt.md → wyciągnij sekcje po nagłówkach "#"
2. Zbuduj user message:
   - intro (hardkod): "Jesteś ekspertem od anonimizacji..."
   - typy encji (hardkod z TYPE_DESCRIPTIONS)
   - zasady (z pliku, sekcja "# Zasady")
   - najczęściej pomijane (z pliku, sekcja "# Najczęściej pomijane")
   - przykład (z pliku, sekcja "# Przykład")
   - tekst dokumentu (runtime)
3. System message → z pliku, sekcja "# System Message"
4. Jeśli plik nie istnieje lub jest uszkodzony → fallback na wbudowany prompt
```

#### Co użytkownik może bezpiecznie edytować:

| Sekcja | Edytowalna | Dlaczego |
|--------|------------|----------|
| System Message | ✅ | Zmiana tonu/języka modelu |
| Zasady | ✅ | Dodanie/usunięcie reguł |
| Najczęściej pomijane | ✅ | Dostosowanie do swoich dokumentów |
| Przykład | ✅ | Lepszy few-shot dla swojego use-case |
| Typy encji | ❌ (hardkod) | Sterują tokenizacją, UI, kategoriami |
| Tekst dokumentu | ❌ (runtime) | Wstawiany dynamicznie per chunk |

#### Fallback:

Brak pliku `prompt.md` → aplikacja działa identycznie jak dziś (wbudowany prompt). Zero breaking change.

## Przyszłe usprawnienia

- **Testy A/B** — porównanie accuracy v1 vs v2 na zestawie testowych dokumentów
