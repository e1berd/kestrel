import type { Operation, Parameters } from './web.js';
import type { ServiceName } from '../services.js';
export type RequestEventType = "request" | "success" | "error" | "settled";
export interface RequestSelector {
    method?: string | readonly string[];
    status?: number | string | readonly (number | string)[];
    url?: string | readonly string[];
    operation?: string | readonly string[];
    service?: ServiceName | readonly ServiceName[];
}
export declare class RequestEvent extends Event {
    readonly operation: Operation;
    readonly params: Parameters;
    readonly request: Request;
    readonly response?: Response;
    readonly data?: unknown;
    readonly error?: unknown;
    readonly durationMs?: number;
    readonly ok: boolean;
    readonly status: number | undefined;
    readonly method: string;
    readonly url: URL;
}
export interface SuccessEvent extends RequestEvent {
    readonly response: Response;
}
export interface FailureEvent extends RequestEvent {
    readonly error: unknown;
}
export interface RequestEventMap {
    request: RequestEvent;
    success: SuccessEvent;
    error: FailureEvent;
    settled: RequestEvent;
}
export type RequestListener<Type extends RequestEventType> = ((event: RequestEventMap[Type]) => void) | {
    handleEvent(event: RequestEventMap[Type]): void;
};
export declare class RequestStream {
    #private;
    constructor(bus: EventTarget, matches: (event: RequestEvent) => boolean);
    filter(selector: RequestSelector | string): RequestStream;
    filter(predicate: (event: RequestEvent) => boolean): RequestStream;
    addEventListener<Type extends RequestEventType>(type: Type, listener: RequestListener<Type>, options?: AddEventListenerOptions | boolean): void;
    removeEventListener<Type extends RequestEventType>(type: Type, listener: RequestListener<Type>): void;
}
export interface RequestEvents {
    emit(type: RequestEventType, detail: Partial<Omit<RequestEvent, keyof Event>>): boolean;
    observe(): RequestStream;
}
export declare function compileSelector(selector: RequestSelector | string): (event: RequestEvent) => boolean;
export declare function createEvents(): RequestEvents;
