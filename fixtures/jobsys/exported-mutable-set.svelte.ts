export function useParams<T>(keys: T[]) {
 const dirty = new Set<T>();
 const add = (key: T) => dirty.add(key);
 return { add };
}