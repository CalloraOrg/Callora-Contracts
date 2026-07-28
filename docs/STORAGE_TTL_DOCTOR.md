# Storage TTL Doctor Utility

The Storage TTL Doctor is a CLI utility that monitors and reports the remaining Time-To-Live (TTL) for each storage key category in the Callora smart contracts by querying their `get_storage_ttl` view endpoints. 

In Soroban, storage entries (such as instance config or developer balances in persistent storage) will automatically be archived if their TTL expires. This utility ensures that operators can monitor the health of their contract storage and trigger extensions (bumps) before data is archived.

---

## View Endpoints in Smart Contracts

Each contract exposes a read-only endpoint `get_storage_ttl`:
- **Vault**: `get_storage_ttl(request_ids: Vec<Symbol>) -> Vec<StorageEntryTtl>`
- **Settlement**: `get_storage_ttl(developer_addresses: Vec<Address>) -> Vec<StorageEntryTtl>`
- **Revenue Pool**: `get_storage_ttl() -> Vec<StorageEntryTtl>`

The returned entries contain the category, description, storage type, current remaining TTL (in ledgers), threshold limit, and bump extension amount.

---

## How to Run Locally

### 1. Install Node.js Dependencies

Run the following command at the root of the project:

```bash
npm install
```

### 2. Run the Doctor Script

You can execute the script using `ts-node` or npm run scripts:

```bash
npx ts-node scripts/storage-ttl-doctor.ts \
  --vault-id "C..." \
  --settlement-id "C..." \
  --revenue-pool-id "C..." \
  --threshold 100000
```

---

## CLI Options

| Option | Description | Default |
|--------|-------------|---------|
| `--threshold <number>` | Min remaining TTL (in ledgers) below which the script exits with code 1 | Uses contract default thresholds |
| `--rpc-url <string>` | Soroban RPC server endpoint | `https://soroban-testnet.stellar.org` |
| `--vault-id <string>` | Contract ID of the deployed Callora Vault | `null` |
| `--settlement-id <string>` | Contract ID of the deployed Callora Settlement | `null` |
| `--revenue-pool-id <string>` | Contract ID of the deployed Callora Revenue Pool | `null` |
| `--request-ids <list>` | Comma-separated list of transaction request IDs to query processed status TTL | `[]` |
| `--developer-addresses <list>`| Comma-separated list of developer addresses to check persistent balance TTL | `[]` (falls back to index) |

---

## JSON Schema

The tool outputs a machine-readable JSON report to stdout:

```json
{
  "timestamp": "2026-06-28T00:10:00.000Z",
  "threshold": 100000,
  "summary": {
    "total_categories": 5,
    "categories_below_threshold": 0,
    "status": "OK"
  },
  "categories": {
    "Instance": {
      "storage_type": "Instance",
      "remaining_ttl": 518400,
      "threshold": 518400,
      "bump_amount": 1036800,
      "status": "OK",
      "entries": [
        {
          "contract": "Vault",
          "contract_id": "CDVAULT...",
          "key_desc": "Instance",
          "ttl": 518400,
          "threshold": 518400,
          "bump_amount": 1036800
        }
      ]
    },
    "ProcessedRequest": {
      "storage_type": "Persistent",
      "remaining_ttl": null,
      "threshold": null,
      "bump_amount": null,
      "status": "EMPTY",
      "entries": []
    }
  },
  "errors": []
}
```

### Exit Codes

- **`0`**: Success (all active categories are above the threshold, no RPC/simulation errors).
- **`1`**: Failure (one or more active categories are below the threshold, or an RPC/simulation error occurred).

---

## Nightly Workflow

The Storage TTL Doctor is configured to run on a nightly cron schedule in `.github/workflows/ttl-doctor.yml`. It runs at 2:00 AM UTC every night, queries the deployed contract addresses configured in GitHub Secrets, and outputs the status report to the action logs.
