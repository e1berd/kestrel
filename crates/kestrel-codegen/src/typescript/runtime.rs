use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) fn files() -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    insert(&mut files, "web", "import type { ServiceName } from '../services.js';\n", include_str!("../web.ts"));
    insert(&mut files, "runtime", "import type { QueryValue } from './web.js';\n", include_str!("../runtime.ts"));
    insert(
        &mut files,
        "routing",
        concat!(
            "import type { ClientOptions, ServiceOptions, SearchParams, Operation, Parameters, RequestOptions } from './web.js';\n",
            "import { services, type ServiceName } from '../services.js';\n",
            "import { interpolate, appendQuery } from './runtime.js';\n",
        ),
        include_str!("../routing.ts"),
    );
    insert(
        &mut files,
        "auth",
        concat!("import type { RequestContext, ServiceOptions } from './web.js';\n", "import { interpolate } from './runtime.js';\n",),
        include_str!("../auth.ts"),
    );
    insert(
        &mut files,
        "cookies",
        concat!(
            "import type { RequestContext, ServiceOptions, CookieStoreAccess } from './web.js';\n",
            "import { httpUrl, assertHeader } from './routing.js';\n",
        ),
        include_str!("../cookies.ts"),
    );
    insert(
        &mut files,
        "hooks",
        concat!(
            "import type { ClientOptions, ClientPlugin, RequestContext } from './web.js';\n",
            "import type { ServiceName } from '../services.js';\n",
            "import { assertHeader } from './routing.js';\n",
        ),
        include_str!("../hooks.ts"),
    );
    insert(
        &mut files,
        "executor",
        concat!(
            "import { ApiError, type ClientOptions, type Operation, type Parameters, type RequestOptions, type RequestContext } from './web.js';\n",
            "import { primaryService, serviceOptions, httpUrl, operationUrl, operationHeaders, assertSecureTransport, sendsCredentials } from './routing.js';\n",
            "import { authenticate } from './auth.js';\n",
            "import { applyCookieSecurity } from './cookies.js';\n",
            "import { clientPlugins, requestPlugins, responsePlugins, errorPlugins } from './hooks.js';\n",
            "import { encodeBody, interpolate } from './runtime.js';\n",
            "import { services } from '../services.js';\n",
        ),
        include_str!("../executor.ts"),
    );
    insert(
        &mut files,
        "collection",
        concat!(
            "import type { Parameters, Operation, RequestOptions } from './web.js';\n",
            "import type { createExecutor } from './executor.js';\n",
            "import { toSearchParams } from './routing.js';\n",
        ),
        include_str!("../collection.ts"),
    );
    files
}

fn insert(files: &mut BTreeMap<PathBuf, String>, name: &str, imports: &str, template: &str) {
    let mut source = imports.to_string();
    source.push('\n');
    for line in template.lines() {
        if line.starts_with("function ")
            || line.starts_with("async function ")
            || line.starts_with("interface Parameters")
            || line.starts_with("interface CollectionOptions")
            || line.starts_with("type Executor")
            || line.starts_with("const resourceExecutor")
        {
            source.push_str("export ");
        }
        source.push_str(line);
        source.push('\n');
    }
    files.insert(PathBuf::from(format!("lib/{name}.ts")), source);
}
