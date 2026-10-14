/** Fetches rows in fixed-size pages and keeps them in memory; one request per page at a time. */
export class RowCache<Row> {
  private pages = new Map<number, Row[]>();
  private inflight = new Set<number>();

  constructor(
    private readonly fetchPage: (offset: number, limit: number) => Promise<Row[]>,
    private readonly total: number,
    private readonly pageSize = 500,
  ) {}

  get(index: number): Row | undefined {
    return this.pages.get(Math.floor(index / this.pageSize))?.[index % this.pageSize];
  }

  has(start: number, end: number): boolean {
    for (let p = this.page(start); p <= this.page(end - 1); p++) {
      if (!this.pages.has(p)) return false;
    }
    return true;
  }

  /** Requests every missing page overlapping `[start, end)`; calls `onLoad` as pages arrive. */
  ensure(start: number, end: number, onLoad: () => void): void {
    const from = this.page(Math.max(0, start));
    const to = this.page(Math.min(this.total, end) - 1);
    for (let p = from; p <= to; p++) {
      if (this.pages.has(p) || this.inflight.has(p)) continue;
      this.inflight.add(p);
      this.fetchPage(p * this.pageSize, this.pageSize)
        .then((rows) => {
          this.pages.set(p, rows);
          onLoad();
        })
        .catch((e: unknown) => console.error(`graph page ${p} failed`, e))
        .finally(() => this.inflight.delete(p));
    }
  }

  private page(index: number): number {
    return Math.max(0, Math.floor(index / this.pageSize));
  }
}
