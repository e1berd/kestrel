import type { ServiceName } from '../services.js';
export type JsonValue = null | boolean | number | string | JsonValue[] | {
    [key: string]: JsonValue;
};
export type QueryScalar = string | number | boolean;
export type QueryValue = QueryScalar | readonly QueryScalar[] | null | undefined;
export type SearchParams = URLSearchParams | string | Iterable<readonly [string, string]> | Record<string, string>;
export interface CookieStoreAccess {
    get(name: string): Promise<{
        name: string;
        value: string;
    } | null>;
}
export interface CookieOptions {
    store?: CookieStoreAccess;
    csrf?: false | {
        cookie: string;
        header: string;
        allowedOrigins?: readonly (string | URL)[];
    };
}
export interface ServiceOptions {
    baseUrl?: string | URL;
    variables?: Readonly<Record<string, string>>;
    headers?: HeadersInit;
    token?: string | ((context: RequestContext) => string | Promise<string>);
    username?: string;
    password?: string;
    apiKey?: string;
    credentials?: RequestCredentials;
    cookies?: CookieOptions;
    allowInsecureAuth?: boolean;
    plugins?: readonly ClientPlugin[];
}
export interface ClientOptions extends ServiceOptions {
    defaultService?: ServiceName;
    queryScope?: string;
    services?: Partial<Record<ServiceName, ServiceOptions>>;
    fetch?: typeof globalThis.fetch;
}
export interface RequestOptions {
    headers?: HeadersInit;
    signal?: AbortSignal;
    credentials?: "omit";
    cache?: RequestCache;
    redirect?: "error" | "manual";
    integrity?: string;
}
export interface RequestContext {
    readonly operation: Operation;
    readonly service: ServiceName;
    readonly url: URL;
    readonly signal?: AbortSignal;
}
export interface ClientPlugin {
    readonly name: string;
    onRequest?(request: Request, context: RequestContext): Request | void | Promise<Request | void>;
    onResponse?(response: Response, request: Request, context: RequestContext): Response | void | Promise<Response | void>;
    onError?(error: unknown, request: Request, context: RequestContext): void | Promise<void>;
}
export declare function defineClientPlugin<const Plugin extends ClientPlugin>(plugin: Plugin): Readonly<Plugin>;
export declare class ApiError extends Error {
    readonly response: Response;
    readonly body: string;
    readonly status: number;
    constructor(response: Response, body: string);
}
export interface Authentication {
    readonly kind: string;
    readonly key: string;
    readonly placement: string;
    readonly prefix: string;
    readonly token: string;
    readonly username: string;
    readonly password: string;
}
export interface Operation {
    readonly id: string;
    readonly service: ServiceName;
    readonly method: string;
    readonly origin: string;
    readonly path: string;
    readonly pathParameters: readonly {
        readonly name: string;
        readonly wire: string;
    }[];
    readonly query: readonly (readonly [string, string])[];
    readonly headers: Readonly<Record<string, string>>;
    readonly sensitiveHeaders: readonly string[];
    readonly sensitiveQuery: readonly string[];
    readonly bodyKind: string;
    readonly auth: Authentication | null;
}
export interface Parameters {
    query?: Record<string, QueryValue>;
    search?: SearchParams;
    body?: unknown;
}
export declare function defineOperation<const Definition extends Operation>(definition: Definition): Definition;
