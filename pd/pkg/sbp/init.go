package sbp

import (
	"github.com/google/uuid"
)

func init() {
	// Enable random pool for uuid, used to generate trace id.
	uuid.EnableRandPool()
}
