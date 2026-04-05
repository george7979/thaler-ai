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

## Decyzje do podjecia

1. Czy losowe kwoty powinny zachowywac rzad wielkosci oryginalnej kwoty? (np. oryginalna 1000-9999 → losowa tez 4-cyfrowa)
2. Czy kwoty z groiszami (1 108,50) powinny dostawac losowe grosze?
3. Czy anonimizowac tez kwoty w sharedStrings (tekst w komorkach typu string)?

---

*Ten dokument zostanie zaktualizowany przed implementacja.*
