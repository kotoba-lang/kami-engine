# kami-hydrogen-electrolysis-sim

Kami Engine side simulation package for high-efficiency hydrogen electrolysis.

It compares:

- commercial alkaline reference
- Hysata-like capillary-fed electrolysis
- capillary-fed + zero-gap AEM + direct high pressure
- SOEC with useful external heat

The model is intentionally deterministic and stdlib-only. It computes Faraday-law hydrogen production, cell voltage loss decomposition, electrical `kWh/kg-H2`, heat-inclusive `kWh/kg-H2`, and optional Isaac/USD scene specs.

```bash
python3 -m pytest tests
```

or without pytest:

```bash
python3 tests/test_model.py
```
