import { type ClientOptions, type Operation, type Parameters, type RequestOptions } from './web.js';
export declare function createExecutor(options: ClientOptions): (<Result>(operation: Operation, params: Parameters, request: RequestOptions) => Promise<Result>) & {
    request: (operation: Operation, params: Parameters, request: RequestOptions) => Promise<Request>;
    response: (operation: Operation, params: Parameters, request: RequestOptions) => Promise<Response>;
};
