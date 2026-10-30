import { createStore, type StoreSchema } from "@rstore/vue";
import { createRstoreCollections, type UsersItem } from "./client.js";

const rstoreSchema = createRstoreCollections(client) satisfies StoreSchema;
const rstore = await createStore({ schema: rstoreSchema, plugins: [] });
const rstoreUsers = await rstore.users.query((query) => query.many());
const rstoreUser: UsersItem | undefined | null = rstoreUsers.data.value[0];
await rstore.users.create({ id: 1, name: "Ada" });
void rstoreUser;
