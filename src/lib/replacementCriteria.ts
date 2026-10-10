export type ReplacementCriterion = {
  id: string;
  description: string;
  met: boolean;
};

export type ReplacementCriteria = {
  criteria: ReplacementCriterion[];
};

export const DEFAULT_REPLACEMENT_CRITERIA: ReplacementCriteria = {
  criteria: [
    { id: 'tests', description: 'All tests pass', met: false },
    { id: 'benchmarks', description: 'Performance benchmarks meet targets', met: false },
    { id: 'security', description: 'Security audit passes', met: false },
    { id: 'packaging', description: 'Packaging artifacts build', met: false },
    { id: 'ci', description: 'CI workflow passes', met: false },
    { id: 'install', description: 'Omarchy install script works', met: false },
  ],
};

export function meetsAll(criteria: ReplacementCriteria): boolean {
  return criteria.criteria.length > 0 && criteria.criteria.every((item) => item.met === true);
}

export function unmetCount(criteria: ReplacementCriteria): number {
  return criteria.criteria.filter((item) => !item.met).length;
}
