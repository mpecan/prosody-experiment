// The page's data, once it has been fetched (`site.ts`). Set once at start-up, before anything
// renders or compiles; every module reads it from here.
import type { SiteData } from './site';

export let site: SiteData;

export function setSite(data: SiteData): void {
  site = data;
}
