function interpolate(template: string, variables: Readonly<Record<string, string>>): string {
  let current = template;
  for (let depth = 0; depth < 20; depth++) {
    if (!current.includes("{{")) return current;
    const next = current.replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (_, name: string) => {
      if (!Object.hasOwn(variables, name)) throw new Error(`Missing SDK variable: ${name}`);
      return variables[name];
    });
    if (next === current) throw new Error("Cyclic or malformed SDK variable template");
    current = next;
  }
  throw new Error("SDK variable expansion exceeded 20 levels");
}

function appendQuery(url: URL, name: string, value: QueryValue): void {
  if (value === null || value === undefined) return;
  for (const item of Array.isArray(value) ? value : [value]) {
    url.searchParams.append(name, String(item));
  }
}

function encodeBody(kind: string, body: unknown, headers: Headers): BodyInit | undefined {
  if (kind === "none") return undefined;
  if (body === undefined) throw new Error("This SDK method requires body");
  if (body instanceof URLSearchParams) {
    if (!headers.has("content-type")) headers.set("content-type", "application/x-www-form-urlencoded");
    return body;
  }
  if (body instanceof FormData) {
    headers.delete("content-type");
    return body;
  }
  if (body instanceof Blob) {
    if (body.type && !headers.has("content-type")) headers.set("content-type", body.type);
    return body;
  }
  if (body instanceof ArrayBuffer || ArrayBuffer.isView(body) || body instanceof ReadableStream) return body as BodyInit;
  let contentType: string | undefined;
  let encoded: BodyInit;
  switch (kind) {
    case "json":
    case "graphql":
      contentType = "application/json";
      encoded = JSON.stringify(body);
      break;
    case "form-urlencoded":
      contentType = "application/x-www-form-urlencoded";
      encoded = body instanceof URLSearchParams ? body : new URLSearchParams(body as Record<string, string>);
      break;
    case "multipart-form":
      if (!(body instanceof FormData)) throw new Error("Multipart body must be FormData");
      headers.delete("content-type");
      return body;
    default:
      contentType = kind === "xml" ? "application/xml" : kind === "sparql" ? "application/sparql-query" : "text/plain";
      encoded = String(body);
  }
  if (!headers.has("content-type")) headers.set("content-type", contentType);
  return encoded;
}
