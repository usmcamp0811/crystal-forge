// Compares full certificate DER identities under Go's AppendCertsFromPEM rules.
// Only counts and digests are emitted. Subjects are not certificate identities.
package main

import (
	"bytes"
	"crypto/sha256"
	"crypto/x509"
	"encoding/asn1"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"os"
	"os/exec"
	"sort"
	"strings"
)

func require(ok bool) {
	if !ok {
		panic("CA bundle regression failed")
	}
}

func read(path string) []byte {
	b, err := os.ReadFile(path)
	require(err == nil)
	return b
}

// Mirror the standard library's eligibility predicate, then use a real pool
// to confirm each selected object is accepted. Deduplicate by complete Raw DER.
func identities(input []byte) []string {
	pool := x509.NewCertPool()
	require(pool.AppendCertsFromPEM(input))
	ids := map[string]bool{}
	for {
		block, rest := pem.Decode(input)
		if block == nil {
			break
		}
		input = rest
		if block.Type != "CERTIFICATE" || len(block.Headers) != 0 {
			continue
		}
		cert, err := x509.ParseCertificate(block.Bytes)
		if err != nil {
			continue
		}
		require(x509.NewCertPool().AppendCertsFromPEM(pem.EncodeToMemory(block)))
		digest := sha256.Sum256(cert.Raw)
		ids[hex.EncodeToString(digest[:])] = true
	}
	result := make([]string, 0, len(ids))
	for id := range ids {
		result = append(result, id)
	}
	sort.Strings(result)
	return result
}

func categories(input []byte) map[string]int {
	c := map[string]int{"cr_lines": 0, "rejected_headings": 0, "comments": 0}
	inside := false
	for _, line := range bytes.Split(bytes.TrimSuffix(input, []byte("\n")), []byte("\n")) {
		s := string(line)
		c["lines"]++
		switch {
		case bytes.Contains(line, []byte("\r")):
			c["cr_lines"]++
		case s == "-----BEGIN CERTIFICATE-----":
			c["plain"]++
			c["framing"]++
			inside = true
		case s == "-----BEGIN TRUSTED CERTIFICATE-----":
			c["trusted"]++
			c["framing"]++
			inside = true
		case s == "-----END CERTIFICATE-----" || s == "-----END TRUSTED CERTIFICATE-----":
			c["framing"]++
			inside = false
		case inside:
			c["base64"]++
		case s == "":
			c["blank"]++
		case bytes.HasPrefix(line, []byte("#")):
			c["comments"]++
		case s == "Trusted for:":
			c["trusted_headings"]++
		case s == "Rejected for:":
			c["rejected_headings"]++
		case bytes.HasPrefix(line, []byte("  - ")):
			c["oid_lines"]++
			descriptions := []string{"RFC5280: serverAuth key usage", "RFC5280: clientAuth key usage", "RFC5280: codeSigning key usage", "RFC5280: emailProtection key usage", "RFC2459: ipsecEndSystem key usage", "RFC2459: ipsecTunnel key usage", "RFC2459: ipsecUser key usage", "RFC5280: timeStamping key usage"}
			known := false
			for i, desc := range descriptions {
				if s == fmt.Sprintf("  - 1.3.6.1.5.5.7.3.%d (%s)", i+1, desc) {
					known = true
				}
			}
			if !known {
				c["unsupported_oid_text"]++
			}
		case s == "Traditional PEM block omitted: this certificate is not trusted for authenticating servers.":
			c["omission_notes"]++
		default:
			c["labels"]++
			if strings.Contains(s, "BEGIN") || strings.Contains(s, "END") {
				c["labels_marker_word"]++
			}
			if strings.Contains(s, "://") || strings.Contains(s, "=") || strings.HasPrefix(s, "-") {
				c["labels_forbidden_shape"]++
			}
			if _, err := base64.StdEncoding.DecodeString(s); err == nil {
				c["labels_decodable_base64"]++
			}
		}
	}
	// Reconstruct the old ordinary-only line parser: skip blanks/comments,
	// accept ordinary markers/body; every other nonempty line is unexpected.
	plain := false
	for _, line := range bytes.Split(bytes.TrimSuffix(input, []byte("\n")), []byte("\n")) {
		s := string(line)
		if s == "-----BEGIN CERTIFICATE-----" {
			plain = true
			continue
		}
		if s == "-----END CERTIFICATE-----" {
			plain = false
			continue
		}
		if plain {
			c["plain_body_lines"]++
			continue
		}
		if len(line) == 0 || bytes.HasPrefix(line, []byte("#")) {
			continue
		}
		c["ordinary_only_unexpected"]++
	}
	return c
}

func main() {
	if len(os.Args) == 2 {
		input := read(os.Args[1])
		require(json.NewEncoder(os.Stdout).Encode(map[string]any{"public": categories(input), "go_unique_roots": len(identities(input))}) == nil)
		return
	}
	require(len(os.Args) == 4 || len(os.Args) == 5)
	public, original, root := read(os.Args[1]), read(os.Args[2]), read(os.Args[len(os.Args)-1])
	c := categories(public)
	require(c["plain"] == 121 && c["trusted"] == 172 && c["blank"] == 293 && c["cr_lines"] == 0)
	require(c["labels"]+c["omission_notes"]+c["trusted_headings"]+c["rejected_headings"]+c["oid_lines"] == 605)
	require(c["ordinary_only_unexpected"] == 4942)
	a, r := identities(original), identities(root)
	require(len(r) == 1)
	require(sort.SearchStrings(a, r[0]) < len(a) && a[sort.SearchStrings(a, r[0])] == r[0])
	require(len(a) == len(identities(public))+1)
	require(!bytes.Contains(original, []byte("PRIVATE KEY")))
	// Parse every public ordinary and AUX certificate, independent of dates.
	remaining, validated := public, 0
	for {
		block, rest := pem.Decode(remaining)
		if block == nil {
			break
		}
		remaining = rest
		require(block.Type == "CERTIFICATE" || block.Type == "TRUSTED CERTIFICATE")
		var object asn1.RawValue
		_, err := asn1.Unmarshal(block.Bytes, &object)
		require(err == nil)
		_, err = x509.ParseCertificate(object.FullBytes)
		require(err == nil)
		command := exec.Command("openssl", "x509", "-noout")
		command.Stdin = bytes.NewReader(pem.EncodeToMemory(block))
		require(command.Run() == nil)
		validated++
	}
	digest := sha256.Sum256([]byte(fmt.Sprint(a)))
	report := map[string]any{"public": categories(public), "onboarding": categories(original),
		"go_unique_roots": len(a), "all_public_x509_valid": validated,
		"root_set_sha256": hex.EncodeToString(digest[:]), "synthetic_root_sha256": r[0], "private_key_blocks": 0}
	if len(os.Args) == 5 {
		normalized := read(os.Args[3])
		require(fmt.Sprint(a) == fmt.Sprint(identities(normalized)))
		require(bytes.Count(normalized, []byte("-----BEGIN CERTIFICATE-----")) == len(a))
		canonical := categories(normalized)
		require(canonical["trusted"] == 0 && canonical["labels"] == 0 && canonical["oid_lines"] == 0)
		require(canonical["blank"] == 0 && canonical["comments"] == 0 && canonical["cr_lines"] == 0)
		require(canonical["ordinary_only_unexpected"] == 0 && !bytes.Contains(normalized, []byte("PRIVATE KEY")))
		report["normalized"] = canonical
		report["identities_equal"] = true
	}
	require(json.NewEncoder(os.Stdout).Encode(report) == nil)
}
