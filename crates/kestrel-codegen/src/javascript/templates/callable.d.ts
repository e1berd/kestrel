import type { Operation, Parameters, RequestOptions } from './web.js';
import type { CollectionOptions, Executor } from './collection.js';
export interface BaseCallable {
    (params?: Parameters, options?: RequestOptions): unknown;
    readonly operation: Operation;
    request(params?: Parameters, options?: RequestOptions): Promise<Request>;
    response(params?: Parameters, options?: RequestOptions): Promise<Response>;
}
export declare function createCallable<Callable = BaseCallable>(execute: Executor, operation: Operation, collection: CollectionOptions | null): Callable;
