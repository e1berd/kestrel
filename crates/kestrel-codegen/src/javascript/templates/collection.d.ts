import type { Parameters, Operation, RequestOptions } from './web.js';
import type { createExecutor } from './executor.js';
export interface Page<Item> {
    readonly items: readonly Item[];
    readonly nextPageToken?: string;
    readonly response: unknown;
}
export declare class Collection<Item> implements AsyncIterable<Item> {
    #private;
    constructor(loadPage: (previous?: Page<Item>) => Promise<Page<Item>>, signal?: AbortSignal);
    pages(): AsyncGenerator<Page<Item>>;
    [Symbol.asyncIterator](): AsyncGenerator<Item>;
}
export interface CollectionOptions {
    resourceName: string;
    items: string | null;
    pageToken: string | null;
    nextPageToken: string | null;
}
export declare function responseField(value: unknown, path: string): unknown;
export declare function decodePage<Item>(response: unknown, options: CollectionOptions): Page<Item>;
export declare function createCollection<Item>(execute: Executor, operation: Operation, params: Parameters, request: RequestOptions, options: CollectionOptions): Collection<Item>;
export type Executor = ReturnType<typeof createExecutor>;
export declare const resourceExecutor: unique symbol;
export declare abstract class Resource {
    protected readonly [resourceExecutor]: Executor;
    constructor(execute: Executor);
}
