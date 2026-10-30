import type { RequestContext, ServiceOptions } from './web.js';
export declare function basicCredentials(username: string, password: string): string;
export declare function authenticate(context: RequestContext, settings: ServiceOptions, variables: Readonly<Record<string, string>>, headers: Headers): Promise<void>;
