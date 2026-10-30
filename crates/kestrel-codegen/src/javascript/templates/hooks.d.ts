import type { ClientOptions, ClientPlugin, RequestContext } from './web.js';
import type { ServiceName } from '../services.js';
export declare function clientPlugins(options: ClientOptions, service: ServiceName): readonly ClientPlugin[];
export declare function assertRequest(request: Request, expected: Request): void;
export declare function requestPlugins(request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<Request>;
export declare function responsePlugins(response: Response, request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<Response>;
export declare function errorPlugins(error: unknown, request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<void>;
