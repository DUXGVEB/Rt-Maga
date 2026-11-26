import type { CatalogFilterQuery, SiteFilterSchema } from "../types";

export function emptyFilterSchema(): SiteFilterSchema {
  return {
    tags: [],
    extraAxes: [],
    filterEndpoint: null,
    includeTagParam: null,
    excludeTagParam: null,
    commaJoinedTags: false,
    minChaptersParam: null,
    keywordParam: null,
  };
}

export function emptyFilterQuery(): CatalogFilterQuery {
  return {
    query: null,
    includeTags: [],
    excludeTags: [],
    minChapters: null,
    updatedWithin: "any",
    extra: {},
  };
}

export function cloneFilterQuery(q?: CatalogFilterQuery | null): CatalogFilterQuery {
  if (!q) return emptyFilterQuery();
  return {
    query: q.query ?? null,
    includeTags: [...(q.includeTags ?? [])],
    excludeTags: [...(q.excludeTags ?? [])],
    minChapters: q.minChapters ?? null,
    updatedWithin: q.updatedWithin ?? "any",
    extra: { ...(q.extra ?? {}) },
  };
}

export function countActiveFilters(q: CatalogFilterQuery): number {
  let count = q.includeTags.length + q.excludeTags.length;
  if (q.minChapters !== null && q.minChapters !== undefined && q.minChapters > 0) count += 1;
  if (q.updatedWithin && q.updatedWithin !== "any") count += 1;
  for (const val of Object.values(q.extra)) {
    const trimmed = val.trim();
    if (trimmed && trimmed !== "-1" && trimmed !== "0" && trimmed !== "all" && trimmed !== "any") {
      count += 1;
    }
  }
  return count;
}

export function hasActiveFilters(q: CatalogFilterQuery): boolean {
  if (q.query && q.query.trim().length > 0) return true;
  return countActiveFilters(q) > 0;
}

export function filterKeyFor(sourceId?: number | null, url?: string | null): string {
  if (sourceId !== undefined && sourceId !== null) {
    return `src:${sourceId}`;
  }
  if (url) {
    try {
      const normalized = url.includes("://") ? url : `https://${url}`;
      const host = new URL(normalized).hostname.replace(/^(www\.|m\.)/i, "").toLowerCase();
      return `host:${host}`;
    } catch {
      return `raw:${url.toLowerCase()}`;
    }
  }
  return "default";
}

class WebFilterStore {
  private queries = $state<Record<string, CatalogFilterQuery>>({});
  private schemas = $state<Record<string, SiteFilterSchema>>({});

  getQuery(key: string): CatalogFilterQuery {
    return cloneFilterQuery(this.queries[key]);
  }

  setQuery(key: string, query: CatalogFilterQuery) {
    this.queries = {
      ...this.queries,
      [key]: cloneFilterQuery(query),
    };
  }

  resetQuery(key: string) {
    this.queries = {
      ...this.queries,
      [key]: emptyFilterQuery(),
    };
  }

  getSchema(key: string): SiteFilterSchema | null {
    return this.schemas[key] ?? null;
  }

  setSchema(key: string, schema: SiteFilterSchema) {
    this.schemas = {
      ...this.schemas,
      [key]: schema,
    };
  }
}

export const webFilterStore = new WebFilterStore();
