# Analiza: Losowe kwoty zamiast tokenów w XLSX

**Data:** 2026-04-05
**Status:** Analiza wstepna, do implementacji

---

## Problem

Tokeny tekstowe `[TH_KWOTA_001]` w komorkach Excela:
- Lamia formuly — `=SUMA(A1:A10)` nie policzy tekstu
- Lamia formatowanie — komorka numeryczna staje sie tekstowa
- Formatowanie warunkowe, wykresy — wszystko pada

## Propozycja

Zamiast tokenow, wstawiac losowe kwoty liczbowe (6 cyfr, zakres 100000-999999).

### Przyklad:

| Etap | Komorka A1 | Komorka A5 (=SUMA) |
|------|-----------|-------------------|
| Oryginal | 1 108 | =SUMA(A1:A4) → 5 500 |
| Obecny (tokeny) | [TH_KWOTA_001] | =SUMA → BLAD |
| Proponowany (losowe) | 847291 | =SUMA → liczy sie (inna wartosc, ale dziala) |

### Mapa anonimizacji:
```json
{
  "847291": { "original": "1108", "type": "AMOUNT" }
}
```

## Wymagania techniczne

### 1. Format losowej kwoty
- **6 cyfr** (100000-999999) — 900k unikalnych wartosci
- Staly format — latwo rozpoznac w mapie
- Nie powtarza sie w mapie ANI w oryginalnym dokumencie

### 2. Rozroznienie formul od wartosci

W worksheet XML:
```xml
<!-- Wartosc reczna → ANONIMIZUJ -->
<c r="A1"><v>1108</v></c>

<!-- Formula → NIE RUSZAJ wartosci -->
<c r="A5"><f>SUM(A1:A4)</f><v>1108</v></c>
```

**Zasada:** Komorka z `<f>` = formula → nie anonimizuj wartosci. Komorka bez `<f>` = reczna → anonimizuj.

### 3. Gdzie zamieniac

- **NIE** w sharedStrings.xml (tam tekst)
- **TAK** w worksheet XML (`xl/worksheets/sheet*.xml`) w tagach `<v>` komorek bez `<f>`
- Wymaga parsowania XML na poziomie komorek, nie bulk replace

### 4. Formatowanie

Oryginal moze byc `1 108,50 PLN` (z waluta, separatorem tysiecy). Losowa wartosc to czysta liczba (`847291`) — formatowanie komorki w Excelu samo doda separatory i walute.

## Ryzyka

- **Sumy sie nie zgadza** — `=SUMA()` policzy losowe kwoty, wynik bedzie inny niz oryginal. Ale lepsze niz tokeny ktore lamia formule calkowicie
- **Proporcje znikaja** — analityk nie zobaczy "ta pozycja jest 10x wieksza od tamtej"
- **Zlozonosc implementacji** — parsowanie worksheet XML na poziomie komorek to wiecej pracy niz obecny bulk replace

## Decyzje (podjete 2026-04-08)

1. **Rzad wielkosci: NIE zachowujemy.** Pelna anonimizacja — staly zakres 100000-999999 (900k unikalnych). Jedyny warunek: losowa wartosc nie moze kolidowac z wartosciami juz obecnymi w arkuszu ani w mapie.
2. **Grosze: NIE generujemy.** Wstawiamy integer. Format komorki w Excelu sam doda `.00` jezeli komorka ma format walutowy/dziesietny. Zero dodatkowej logiki.
3. **SharedStrings: NIE randomizujemy.** Komorki tekstowe (`t="s"`, `t="inlineStr"`) dostaja tokeny `[TH_KWOTA_001]` jak dotychczas. Randomizacja dotyczy wylacznie komorek numerycznych.

## Plan implementacji

### Zakres

Tylko XLSX. W pozostalych formatach (DOCX, TXT, CSV, MD) tokeny tekstowe nie lamia niczego — zostaja bez zmian.

### UI — sub-checkbox pod "Kwoty"

```
☑ Kwoty
   ☐ losowe (XLSX)
```

- Zagniezdzona opcja pod kategoria "Kwoty" (wciety checkbox)
- **Domyslnie odznaczona** — zachowuje obecne zachowanie
- **Wyszarzona** gdy:
  - plik nie jest XLSX, LUB
  - kategoria "Kwoty" jest odznaczona
- Tooltip dla wyszarzonego stanu: "Dostepne tylko dla plikow XLSX z wlaczona kategoria Kwoty"
- Odblokowana automatycznie po zaladowaniu pliku `.xlsx` (jezeli "Kwoty" zaznaczone)

### Logika dzialania

| Typ komorki XLSX | Rozpoznanie w XML | Akcja |
|---|---|---|
| Numeryczna bez formuly | `<c>` z `<v>`, bez `<f>`, bez `t="s"` | Losowa 100000-999999 |
| Formula | `<c>` z `<f>` | Nie ruszac |
| Tekst (shared string) | `<c t="s">` | Token `[TH_KWOTA_001]` jak dotychczas |
| Inline string | `<c t="inlineStr">` | Token jak dotychczas |

### Warstwy systemu

```
NER (Ollama)              Eksport XLSX
─────────────             ────────────────────────
Kategoria "Kwoty"    →    checkbox "losowe (XLSX)"
rozpoznaje kwoty          decyduje JAK je zanonimizowac
w tekscie                 w pliku wyjsciowym
```

- **Kwoty odznaczone** → NER nie szuka kwot → brak kwot w mapie → "losowe" bez znaczenia
- **Kwoty zaznaczone + losowe odznaczone** → tokeny `[TH_KWOTA_001]` (obecne zachowanie)
- **Kwoty zaznaczone + losowe zaznaczone + XLSX** → numeryczne komorki dostaja losowe liczby

### Zmiany w kodzie

1. **`src/index.html`** — sub-checkbox pod "Kwoty", z wcieniem
2. **`src/style.css`** — styl dla zagniezdzonego checkboxa + styl `disabled`
3. **`src/main.js`** — logika wyszarzania (reaguje na zmiane pliku i zmiane checkboxa "Kwoty"), przekazanie flagi do backendu
4. **`src-tauri/src/anonymizer.rs`** — `export_anon_xlsx()`:
   - Nowy parametr: `randomize_amounts: bool`
   - Gdy `true`: w worksheet XML, komorki numeryczne bez formul ktore zawieraja rozpoznana kwote → losowa wartosc zamiast tokena
   - Mapa: `{ "847291": { "original": "1108", "type": "AMOUNT" } }`
   - Pominiecie `fix_xlsx_cell_types` dla tych komorek (zostaja numeryczne)
5. **`src-tauri/src/main.rs`** — endpoint `/api/export-anon-native` przyjmuje flage `randomize_amounts`
6. **Deanonimizacja XLSX** — `deanonymize_xlsx()` musi obslugiwac oba formaty w mapie (tokeny tekstowe + losowe liczby)

### Kolejnosc implementacji

1. UI (checkbox + logika wyszarzania)
2. Backend (parametr w endpoincie)
3. Randomizacja w `export_anon_xlsx()`
4. Deanonimizacja (obsluga losowych kwot w mapie)
5. Testy manualne z przykladowym XLSX z formulami

---

*Decyzje podjete 2026-04-08. Gotowe do implementacji.*
