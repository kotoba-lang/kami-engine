from __future__ import annotations

from .model import ElectrolyzerResult


def scene_spec(results: list[ElectrolyzerResult]) -> dict:
    max_h2 = max(result.h2_kg_per_hour for result in results)
    min_energy = min(result.electrical_kwh_per_kg for result in results)
    nodes = []
    for index, result in enumerate(results):
        x = index * 3.0
        nodes.append(
            {
                "id": f"electrolyzer-{result.name}",
                "kind": "electrolyzer-stack",
                "position": [x, 0.0, result.h2_kg_per_hour / max_h2],
                "scale": [0.9, 0.45, max(0.2, result.h2_kg_per_hour / max_h2 * 2.5)],
                "metrics": result.to_dict(),
            }
        )
        nodes.append(
            {
                "id": f"loss-{result.name}",
                "kind": "loss-penalty",
                "position": [x - 0.85, 0.75, 0.0],
                "scale": [0.16, 0.16, max(0.05, (result.electrical_kwh_per_kg - min_energy) / 12.0)],
            }
        )
        nodes.append(
            {
                "id": f"h2-{result.name}",
                "kind": "hydrogen-output",
                "position": [x, -0.95, 1.0],
                "scale": [0.22, 0.22, 0.22],
            }
        )
    return {"scene_units": "meters", "nodes": nodes}
