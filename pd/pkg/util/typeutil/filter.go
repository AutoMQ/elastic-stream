package typeutil

// FilterZero returns a new slice with all zero values removed.
func FilterZero[T comparable](l []T) (res []T) {
	var n T
	res = make([]T, 0, len(l))
	for _, t := range l {
		if t == n {
			continue
		}
		res = append(res, t)
	}
	return
}
