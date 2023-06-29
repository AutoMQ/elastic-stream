package typeutil

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFilterZero(t *testing.T) {
	type s struct {
		i int
	}
	type testCase[T comparable] struct {
		in   []T
		want []T
	}

	t.Parallel()
	re := require.New(t)

	testValue := testCase[s]{
		in:   []s{{i: 0}, {i: 1}, {i: 2}},
		want: []s{{i: 1}, {i: 2}},
	}
	testPointer := testCase[*s]{
		in:   []*s{nil, {i: 0}, {i: 1}, {i: 2}},
		want: []*s{{i: 0}, {i: 1}, {i: 2}},
	}

	re.Equal(testValue.want, FilterZero[s](testValue.in))
	re.Equal(testPointer.want, FilterZero[*s](testPointer.in))
}
