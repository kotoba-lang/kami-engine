import { describe, expect, it } from 'vitest';
import {
  applySelection,
  initialState,
  applyKpiDelta,
  ZERO_KPI,
  type IncidentScenario,
} from './index.js';

// Tiny synthetic scenario — the SDK is content-free, scenarios live in
// downstream private projects. This fixture only verifies graph invariants.
const TINY: IncidentScenario = {
  id: 'ai.gftd.apps.webvr.test.tiny',
  title: 'tiny',
  synopsis: 'tiny test scenario',
  start: 'a',
  nodes: {
    a: {
      id: 'a',
      stage: 'detect',
      severity: 'high',
      location: 'scadaRoom',
      briefing: 'choose',
      choices: [
        { id: 'good', label: 'good', next: 'win',  delta: { mttdSec: 10 },                  grade: 'best', rationale: 'r' },
        { id: 'bad',  label: 'bad',  next: 'lose', delta: { regulatoryRiskPermille: 800 },  grade: 'bad',  rationale: 'r' },
      ],
    },
    win:  { id: 'win',  stage: 'govern',      severity: 'info',     location: 'executiveRoom', briefing: 'ok',    choices: [], terminal: 'success' },
    lose: { id: 'lose', stage: 'communicate', severity: 'critical', location: 'press',         briefing: 'oh no', choices: [], terminal: 'failure' },
  },
};

describe('kami-engine-sdk webvr', () => {
  it('initializes state at scenario start', () => {
    const s = initialState(TINY);
    expect(s.current).toBe('a');
    expect(s.kpi).toEqual(ZERO_KPI);
    expect(s.history).toHaveLength(0);
    expect(s.done).toBe(false);
  });

  it('applies KPI delta on selection and advances current', () => {
    const s0 = initialState(TINY);
    const s1 = applySelection(TINY, s0, 'good');
    expect(s1.current).toBe('win');
    expect(s1.kpi.mttdSec).toBe(10);
    expect(s1.history).toHaveLength(1);
    expect(s1.history[0].choiceId).toBe('good');
    expect(s1.history[0].grade).toBe('best');
    expect(s1.done).toBe(true);
    expect(s1.outcome).toBe('success');
  });

  it('routes a bad choice to a failure terminal', () => {
    const s0 = initialState(TINY);
    const s1 = applySelection(TINY, s0, 'bad');
    expect(s1.current).toBe('lose');
    expect(s1.kpi.regulatoryRiskPermille).toBe(800);
    expect(s1.done).toBe(true);
    expect(s1.outcome).toBe('failure');
  });

  it('does not advance once terminal is reached', () => {
    const s0 = initialState(TINY);
    const s1 = applySelection(TINY, s0, 'good');
    const s2 = applySelection(TINY, s1, 'good');
    expect(s2).toBe(s1); // identity preserved on no-op
  });

  it('throws on unknown choice id', () => {
    const s0 = initialState(TINY);
    expect(() => applySelection(TINY, s0, 'nope')).toThrow(/choice id "nope"/);
  });

  it('clamps regulatoryRiskPermille to 0..1000', () => {
    const high = applyKpiDelta(ZERO_KPI, { regulatoryRiskPermille: 1500 });
    expect(high.regulatoryRiskPermille).toBe(1000);
    const low = applyKpiDelta(ZERO_KPI, { regulatoryRiskPermille: -500 });
    expect(low.regulatoryRiskPermille).toBe(0);
  });

  it('keeps KPI integers (AT lexicon float-discipline invariant)', () => {
    const k = applyKpiDelta(ZERO_KPI, {
      mttdSec: 12, mttrSec: 34, downtimeMin: 5, regulatoryRiskPermille: 250, dataLossGb: 2, costYenDeci: 999,
    });
    for (const v of Object.values(k)) {
      expect(Number.isInteger(v)).toBe(true);
    }
  });

  it('scenario reachability — every non-terminal node reaches some terminal', () => {
    function reachesTerminal(scenario: IncidentScenario, fromId: string, seen = new Set<string>()): boolean {
      if (seen.has(fromId)) return false;
      seen.add(fromId);
      const n = scenario.nodes[fromId];
      if (!n) return false;
      if (n.terminal) return true;
      if (n.choices.length === 0) return false;
      return n.choices.some((c) => reachesTerminal(scenario, c.next, seen));
    }
    for (const id of Object.keys(TINY.nodes)) {
      expect(reachesTerminal(TINY, id)).toBe(true);
    }
  });

  it('scenario integrity — every choice targets an existing node', () => {
    for (const node of Object.values(TINY.nodes)) {
      for (const c of node.choices) {
        expect(TINY.nodes[c.next]).toBeDefined();
      }
    }
  });
});
