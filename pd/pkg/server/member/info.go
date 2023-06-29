package member

import (
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

// Info is member info stored in etcd
type Info struct {
	Name            string   `json:"name"`              // PD server name
	MemberID        uint64   `json:"member_id"`         // Member.id
	PeerUrls        []string `json:"peer_urls"`         // Member.etcd.Config().APUrls
	ClientUrls      []string `json:"client_urls"`       // Member.etcd.Config().ACUrls
	AdvertisePDAddr string   `json:"advertise_pd_addr"` // Config.AdvertisePDAddr
	IsLeader        bool     `json:"-"`                 // Whether current PD node is the leader
}

// MarshalLogObject implements zapcore.ObjectMarshaler
func (i Info) MarshalLogObject(encoder zapcore.ObjectEncoder) error {
	encoder.AddString("name", i.Name)
	encoder.AddUint64("member_id", i.MemberID)
	zap.Strings("peer_urls", i.PeerUrls).AddTo(encoder)
	zap.Strings("client_urls", i.ClientUrls).AddTo(encoder)
	encoder.AddString("advertise_pd_addr", i.AdvertisePDAddr)
	encoder.AddBool("is_leader", i.IsLeader)
	return nil
}
