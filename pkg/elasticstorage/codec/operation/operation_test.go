package operation

import (
	"testing"

	"github.com/stretchr/testify/require"
	"go.uber.org/goleak"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

func TestNewOperation(t *testing.T) {
	type fields struct {
		code Code
	}
	tests := []struct {
		name   string
		fields fields
		want   string
	}{
		{
			name:   "Ping",
			fields: fields{code: ping},
			want:   "Ping",
		},
		{
			name:   "GoAway",
			fields: fields{code: goAway},
			want:   "GoAway",
		},
		{
			name:   "Publish",
			fields: fields{code: publish},
			want:   "Publish",
		},
		{
			name:   "Heartbeat",
			fields: fields{code: heartbeat},
			want:   "Heartbeat",
		},
		{
			name:   "ListRange",
			fields: fields{code: listRange},
			want:   "ListRange",
		},
		{
			name:   "Unknown",
			fields: fields{code: unknown},
			want:   "Unknown",
		},
		{
			name:   "Unknown code",
			fields: fields{code: 42},
			want:   "Unknown",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			o := NewOperation(tt.fields.code)

			re.Equal(tt.want, o.String())
		})
	}
}

func TestOperation(t *testing.T) {
	tests := []struct {
		name   string
		opFunc func() Operation
		want   Operation
	}{
		{
			name:   "Ping",
			opFunc: Ping,
			want:   NewOperation(ping),
		},
		{
			name:   "GoAway",
			opFunc: GoAway,
			want:   NewOperation(goAway),
		},
		{
			name:   "Publish",
			opFunc: Publish,
			want:   NewOperation(publish),
		},
		{
			name:   "Heartbeat",
			opFunc: Heartbeat,
			want:   NewOperation(heartbeat),
		},
		{
			name:   "ListRange",
			opFunc: ListRange,
			want:   NewOperation(listRange),
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			re.Equal(tt.want, tt.opFunc())
		})
	}
}
