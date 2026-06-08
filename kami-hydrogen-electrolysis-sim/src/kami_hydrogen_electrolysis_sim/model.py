from __future__ import annotations

from dataclasses import asdict, dataclass

FARADAY_C_PER_MOL = 96485.33212
H2_KG_PER_MOL = 0.00201588
KWH_PER_J = 1.0 / 3_600_000.0
HHV_KWH_PER_KG_H2 = 39.41
LHV_KWH_PER_KG_H2 = 33.33


@dataclass(frozen=True)
class ElectrolyzerCase:
    name: str
    description: str
    current_density_a_cm2: float
    active_area_cm2: float
    thermoneutral_voltage_v: float
    activation_overvoltage_v: float
    ohmic_loss_v: float
    bubble_transport_loss_v: float
    faradaic_efficiency: float
    auxiliary_kwh_per_kg: float
    output_pressure_bar: float
    heat_input_kwh_per_kg: float = 0.0

    @property
    def cell_voltage_v(self) -> float:
        return (
            self.thermoneutral_voltage_v
            + self.activation_overvoltage_v
            + self.ohmic_loss_v
            + self.bubble_transport_loss_v
        )


@dataclass(frozen=True)
class ElectrolyzerResult:
    name: str
    description: str
    current_a: float
    h2_kg_per_hour: float
    cell_voltage_v: float
    dc_power_kw: float
    electrical_kwh_per_kg: float
    total_with_heat_kwh_per_kg: float
    hhv_electrical_efficiency_pct: float
    hhv_total_efficiency_pct: float
    lhv_electrical_efficiency_pct: float
    output_pressure_bar: float
    heat_input_kwh_per_kg: float

    def to_dict(self) -> dict[str, float | str]:
        return asdict(self)


def electrical_energy_kwh_per_kg(
    cell_voltage_v: float,
    faradaic_efficiency: float,
) -> float:
    mol_h2_per_kg = 1.0 / H2_KG_PER_MOL
    coulombs_per_kg = 2.0 * FARADAY_C_PER_MOL * mol_h2_per_kg
    return cell_voltage_v * coulombs_per_kg * KWH_PER_J / faradaic_efficiency


def simulate_case(case: ElectrolyzerCase) -> ElectrolyzerResult:
    current_a = case.current_density_a_cm2 * case.active_area_cm2
    h2_mol_per_s = current_a * case.faradaic_efficiency / (2.0 * FARADAY_C_PER_MOL)
    h2_kg_per_hour = h2_mol_per_s * H2_KG_PER_MOL * 3600.0
    dc_power_kw = current_a * case.cell_voltage_v / 1000.0
    electrical = electrical_energy_kwh_per_kg(
        case.cell_voltage_v,
        case.faradaic_efficiency,
    ) + case.auxiliary_kwh_per_kg
    total_with_heat = electrical + case.heat_input_kwh_per_kg
    return ElectrolyzerResult(
        name=case.name,
        description=case.description,
        current_a=current_a,
        h2_kg_per_hour=h2_kg_per_hour,
        cell_voltage_v=case.cell_voltage_v,
        dc_power_kw=dc_power_kw,
        electrical_kwh_per_kg=electrical,
        total_with_heat_kwh_per_kg=total_with_heat,
        hhv_electrical_efficiency_pct=HHV_KWH_PER_KG_H2 / electrical * 100.0,
        hhv_total_efficiency_pct=HHV_KWH_PER_KG_H2 / total_with_heat * 100.0,
        lhv_electrical_efficiency_pct=LHV_KWH_PER_KG_H2 / electrical * 100.0,
        output_pressure_bar=case.output_pressure_bar,
        heat_input_kwh_per_kg=case.heat_input_kwh_per_kg,
    )


def default_cases(active_area_cm2: float = 10_000.0) -> list[ElectrolyzerCase]:
    return [
        ElectrolyzerCase(
            name="commercial-alkaline-reference",
            description="Immersed alkaline electrodes with non-trivial bubble coverage.",
            current_density_a_cm2=0.6,
            active_area_cm2=active_area_cm2,
            thermoneutral_voltage_v=1.48,
            activation_overvoltage_v=0.20,
            ohmic_loss_v=0.17,
            bubble_transport_loss_v=0.08,
            faradaic_efficiency=0.98,
            auxiliary_kwh_per_kg=0.20,
            output_pressure_bar=30.0,
        ),
        ElectrolyzerCase(
            name="hysata-like-cfe",
            description="Capillary-fed liquid supply with near bubble-free electrode faces.",
            current_density_a_cm2=1.0,
            active_area_cm2=active_area_cm2,
            thermoneutral_voltage_v=1.48,
            activation_overvoltage_v=0.025,
            ohmic_loss_v=0.018,
            bubble_transport_loss_v=0.003,
            faradaic_efficiency=0.99,
            auxiliary_kwh_per_kg=0.46,
            output_pressure_bar=30.0,
        ),
        ElectrolyzerCase(
            name="cfe-zero-gap-aem-high-pressure",
            description=(
                "Low-temperature hybrid: capillary feed, zero-gap AEM stack, "
                "and direct high-pressure hydrogen output."
            ),
            current_density_a_cm2=2.0,
            active_area_cm2=active_area_cm2,
            thermoneutral_voltage_v=1.48,
            activation_overvoltage_v=0.020,
            ohmic_loss_v=0.010,
            bubble_transport_loss_v=0.002,
            faradaic_efficiency=0.992,
            auxiliary_kwh_per_kg=0.15,
            output_pressure_bar=70.0,
        ),
        ElectrolyzerCase(
            name="soec-useful-heat",
            description="High-temperature steam electrolysis using external heat.",
            current_density_a_cm2=1.0,
            active_area_cm2=active_area_cm2,
            thermoneutral_voltage_v=1.18,
            activation_overvoltage_v=0.06,
            ohmic_loss_v=0.05,
            bubble_transport_loss_v=0.0,
            faradaic_efficiency=0.98,
            auxiliary_kwh_per_kg=1.00,
            output_pressure_bar=20.0,
            heat_input_kwh_per_kg=8.0,
        ),
    ]


def simulate_default_cases(active_area_cm2: float = 10_000.0) -> list[ElectrolyzerResult]:
    return [simulate_case(case) for case in default_cases(active_area_cm2)]


def rank_by_electrical_energy(
    results: list[ElectrolyzerResult],
) -> list[ElectrolyzerResult]:
    return sorted(results, key=lambda result: result.electrical_kwh_per_kg)
