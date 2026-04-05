# Wymagania produktowe - Thaler AI

<!-- ⚠️ CKM SEPARATION WARNING ⚠️ -->
<!-- PRD.md is for BUSINESS FOCUS (WHAT & WHY) -->
<!-- DON'T include: Code snippets, file paths, technical details, timelines -->
<!-- DO include: Business requirements, user needs, success metrics -->
<!-- For implementation details → see TECH.md -->
<!-- For timeline planning → see PLAN.md -->

**Autor:** Jerzy Maczewski
**Cel:** Localhost web app do anonimizacji dokumentów z lokalnym LLM — umożliwia bezpieczną pracę z poufnymi dokumentami w środowiskach AI

---

## Podsumowanie

Thaler AI to aplikacja (localhost web app) do anonimizacji dokumentów przed przetwarzaniem przez chmurowe usługi AI (Claude, GPT, Gemini). Wykorzystuje lokalne modele LLM do wykrywania danych wrażliwych i zastępuje je deterministycznymi tokenami. Po przetworzeniu w chmurze dane są odtwarzane do postaci oryginalnej.

---

## Opis problemu

### Obecne wyzwania:
- **Polityki bezpieczeństwa firm** zabraniają udostępniania poufnych dokumentów do usług AI w chmurze
- **Ręczna anonimizacja** jest czasochłonna, podatna na błędy i nieopłacalna
- **Brak narzędzi** łączących lokalne wykrywanie danych wrażliwych z workflow AI
- **Utrata kontekstu** przy ręcznym zamazywaniu — AI nie rozumie dokumentu z "XXX" zamiast nazw

### Kontekst biznesowy:
- Rosnące zapotrzebowanie na analizę dokumentów z AI w korporacjach i sektorze publicznym
- Regulacje (RODO, NDA, klauzule poufności) ograniczają użycie chmurowego AI
- Lokalne LLM (Ollama) osiągają jakość wystarczającą do wykrywania danych wrażliwych w języku polskim

---

## Użytkownicy docelowi

### Główni użytkownicy:
1. **Analitycy/konsultanci IT** — pracują z poufnymi dokumentami, chcą używać AI do analizy
2. **Freelancerzy B2B** — pracują na dokumentach objętych klauzulami poufności

### Persony użytkowników:
- **Analityk B2B** — pracuje z dokumentami korporacyjnymi oznaczonymi "Do użytku wewnętrznego", chce analizować je z AI bez naruszenia poufności
- **PM w firmie IT** — dostaje specyfikacje od klientów, potrzebuje AI do wyciągania wymagań, ale klient zabrania przetwarzania w chmurze

---

## Cele biznesowe

### Główne cele:
1. **Bezpieczna praca z AI na poufnych dokumentach** — zero danych wrażliwych opuszcza maszynę
2. **Automatyczna anonimizacja** — wykrycie 95%+ danych wrażliwych bez interwencji użytkownika
3. **Pełna odwracalność** — de-anonimizowany dokument identyczny z oryginałem

### Oczekiwane ROI:
- Oszczędność 30-60 min na dokument vs ręczna anonimizacja
- Eliminacja ryzyka wycieku danych do chmury AI
- Umożliwienie pracy z AI tam, gdzie dotąd była zablokowana

---

## Wymagania funkcjonalne

### FR1: Import dokumentów
- **FR1.1** Import plików DOCX (dokumenty Word)
- **FR1.2** Import plików XLSX (arkusze kalkulacyjne)
- **FR1.3** Import plików MD/TXT/CSV (dokumenty tekstowe)
- **FR1.4** Podgląd zawartości importowanego dokumentu

### FR2: Anonimizacja
- **FR2.1** Automatyczne wykrywanie danych wrażliwych (pełna lista typów → TECH.md)
- **FR2.2** Deterministyczne tokeny — ta sama encja zawsze daje ten sam token (TH_FIRMA_001, TH_OSOBA_003)
- **FR2.3** Podgląd wykrytych encji przed zatwierdzeniem (tabela mapowań)
- **FR2.4** Konfiguracja kategorii anonimizacji — checkboxy do wyłączania typów (Osoby, Adresy, Firmy, Numery ID, Kwoty, Kontakt, Daty)
- **FR2.5** Wybór modelu LLM przez użytkownika

### FR3: De-anonimizacja
- **FR3.1** Odtworzenie oryginalnych danych z tokenów
- **FR3.2** Walidacja — sygnalizacja nierozwiązanych tokenów

### FR4: Eksport
- **FR4.1** Eksport w formacie źródłowym (DOCX→DOCX, XLSX→XLSX) z zachowaniem formatowania
- **FR4.2** Zapis do pliku tekstowego (MD)
- **FR4.3** Eksport mapowania token ↔ oryginał (.map.json)
- **FR4.4** Podgląd tabeli mapowań w UI

---

## Wymagania niefunkcjonalne

### Wydajność:
- **NFR1** Anonimizacja dokumentu <5 stron w czasie <60s (zależy od modelu LLM)
- **NFR2** De-anonimizacja natychmiastowa (<1s)

### Użyteczność:
- **NFR3** Intuicyjny UI — dialog plikowy, jeden przycisk do anonimizacji
- **NFR4** Ciemny/jasny motyw z autodetekcją systemową
- **NFR5** Polski interfejs

### Bezpieczeństwo:
- **NFR6** Zero danych wrażliwych opuszcza maszynę — przetwarzanie wyłącznie lokalne
- **NFR7** Brak telemetrii, brak plików konfiguracyjnych, brak połączeń zewnętrznych
- Szczegóły mechanizmów bezpieczeństwa → TECH.md

### Przenośność:
- **NFR10** Dostępność na Linux i Windows
- **NFR11** Standalone binary — zero zależności runtime

---

## Metryki sukcesu

- **M1** **Skuteczność wykrywania**: >95% danych wrażliwych wykrytych automatycznie
- **M2** **Wierność cyklu**: 100% — de-anonimizowany dokument identyczny z oryginałem
- **M3** **Czas anonimizacji**: <60s dla dokumentu 5-stronicowego
- **M4** **Rozmiar pakietu**: <10 MB

---

## Ograniczenia i założenia

- Projekt open-source (publiczne repo), jednoosobowy development
- Brak budżetu na komercyjne API/narzędzia
- Wymaga dostępu do lokalnego serwera LLM (szczegóły → TECH.md)
- Dokumenty wejściowe to tekst strukturalny — nie skany PDF
- Dane wrażliwe w dokumentach są w języku polskim

---

## Planowane rozszerzenia

Szczegółowy harmonogram i priorytety → PLAN.md

---

*Ten PRD definiuje wymagania biznesowe dla Thaler AI. Wszystkie decyzje implementacyjne powinny być walidowane względem tych wymagań.*
