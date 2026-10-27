// The types of deploy/preview.js, for the page's tests that check a link made by the page reads
// back in the worker.
export const MOST: number;
export function sharedIn(url: string): Promise<{ poem: string; style: string | null } | null>;
export function previewOf(shared: { poem: string; style: string | null }, canonical?: string): { title: string; description: string };
export function escape(text: string): string;
export function withPreview(html: string, preview: { title: string; description: string }, url: string): string;
