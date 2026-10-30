const query = client.users.get.queryOptions<User>({ userId: '42' });
const queryResult: Promise<User> = query.queryFn({ signal: new AbortController().signal });
const listQuery = client.users.list.queryOptions<User>();
const listResult: Promise<User[]> = listQuery.queryFn({ signal: new AbortController().signal });
const mutation = client.users.insert.mutationOptions<User>();
const mutationResult: Promise<User> = mutation.mutationFn({ body: { name: 'Ada' } });
void queryResult;
void listResult;
void mutationResult;
