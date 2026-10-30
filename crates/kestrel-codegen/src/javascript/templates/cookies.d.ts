import type { RequestContext, ServiceOptions } from './web.js';
export declare function applyCookieSecurity(context: RequestContext, settings: ServiceOptions, headers: Headers, credentials: RequestCredentials): Promise<void>;
