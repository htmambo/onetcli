export function nowIso(): string {
  return new Date().toISOString();
}

export function plusHoursIso(hours: number): string {
  return new Date(Date.now() + hours * 60 * 60 * 1000).toISOString();
}

export function parseSince(input: string | undefined): string | undefined {
  if (!input) {
    return undefined;
  }

  if (/^\d+$/.test(input)) {
    const millis = Number(input);
    if (Number.isFinite(millis)) {
      return new Date(millis).toISOString();
    }
  }

  const date = new Date(input);
  if (Number.isNaN(date.getTime())) {
    return undefined;
  }

  return date.toISOString();
}
