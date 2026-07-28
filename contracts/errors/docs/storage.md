# Errors Contract Storage Documentation

This document outlines the storage keys used by the Errors smart contract, detailing their Soroban storage tiers and the rationale behind their selection.

## Storage Keys Hierarchy

### `DataKey::Admin`
* **Tier:** **Instance**
* **Rationale:** The administrator's address dictates the access control for the entire contract. Because it is globally applicable to the contract and must survive as long as the contract is active, the `Instance` tier is used. This ensures the admin data shares the same Time-To-Live (TTL) as the contract instance itself, preventing access lockouts.

### `DataKey::ErrorRegistry(u32)`
* **Tier:** **Persistent**
* **Rationale:** This key stores the mapping of specific error codes to their detailed string descriptions. Since this acts as a core registry that protocols and frontends rely on to decode errors, it must be durably stored. `Persistent` storage guarantees that the state cannot be arbitrarily dropped without an explicit archival process, ensuring high availability.

### `DataKey::RecentError(Address)`
* **Tier:** **Temporary**
* **Rationale:** This key tracks the most recent error logged by a specific user address for short-term debugging or rate-limiting. Since this data becomes irrelevant quickly, it uses `Temporary` storage. This significantly reduces state bloat and protocol fees, as the network can naturally drop the data once the TTL expires.