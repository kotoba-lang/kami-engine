from __future__ import annotations

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from kami_hydrogen_electrolysis_sim import rank_by_electrical_energy, simulate_default_cases
from kami_hydrogen_electrolysis_sim.usd import scene_spec


def test_cfe_improves_on_reference():
    by_name = {result.name: result for result in simulate_default_cases()}
    assert by_name["hysata-like-cfe"].electrical_kwh_per_kg < by_name["commercial-alkaline-reference"].electrical_kwh_per_kg


def test_low_temperature_best_candidate_is_cfe_zero_gap_aem_high_pressure():
    low_temp = [
        result
        for result in simulate_default_cases()
        if not result.name.startswith("soec")
    ]
    assert rank_by_electrical_energy(low_temp)[0].name == "cfe-zero-gap-aem-high-pressure"


def test_soec_heat_inclusive_energy_is_higher_than_electrical_only():
    soec = {result.name: result for result in simulate_default_cases()}["soec-useful-heat"]
    assert soec.total_with_heat_kwh_per_kg > soec.electrical_kwh_per_kg


def test_scene_spec_contains_three_nodes_per_case():
    results = simulate_default_cases()
    spec = scene_spec(results)
    assert spec["scene_units"] == "meters"
    assert len(spec["nodes"]) == len(results) * 3


if __name__ == "__main__":
    tests = [value for name, value in sorted(globals().items()) if name.startswith("test_")]
    for test in tests:
        test()
    print(f"{len(tests)}/{len(tests)} passed")
