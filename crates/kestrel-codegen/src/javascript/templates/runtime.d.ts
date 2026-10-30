import type { QueryValue } from './web.js';
export declare function interpolate(template: string, variables: Readonly<Record<string, string>>): string;
export declare function appendQuery(url: URL, name: string, value: QueryValue): void;
export declare function encodeBody(kind: string, body: unknown, headers: Headers): BodyInit | undefined;
