"""
Temporal Graph Network (TGN) Training and ONNX Export for AegisGraph.
Trains on IBM AML transaction data with continuous-time node memory
and dynamic edge anomaly scoring.
"""

import os
import math
import time
import csv
import datetime as dt
import numpy as np
import torch
import torch.nn as nn
import torch.optim as optim

class TimeEncoder(nn.Module):
    """Continuous harmonic Fourier time encoding: phi(dt) = cos(dt * omega + b)."""
    def __init__(self, time_dim=16):
        super().__init__()
        self.time_dim = time_dim
        # Log-spaced frequencies from 1e-6 to 1.0 (covers 1 sec to ~11 days)
        frequencies = 2.0 * math.pi * torch.exp(torch.linspace(math.log(1e-6), math.log(1.0), time_dim))
        self.register_buffer("frequencies", frequencies)

    def forward(self, dt_seconds):
        # dt_seconds: [B, 1]
        angles = dt_seconds * self.frequencies.unsqueeze(0) # [B, time_dim]
        return torch.cos(angles)

class TGNScorerModel(nn.Module):
    """
    Continuous-Time Dynamic Graph Scorer.
    Takes source & target node memory states, time delta, and edge features,
    predicts laundering risk probability in [0, 1], and outputs updated memory states.
    """
    def __init__(self, mem_dim=32, edge_dim=8, time_dim=16, hidden_dim=64):
        super().__init__()
        self.mem_dim = mem_dim
        self.time_encoder = TimeEncoder(time_dim)

        in_dim = mem_dim * 2 + time_dim + edge_dim

        # Anomaly scoring network
        self.mlp = nn.Sequential(
            nn.Linear(in_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, 32),
            nn.ReLU(),
            nn.Linear(32, 1),
            nn.Sigmoid()
        )

        # GRU memory updaters for source and target
        self.src_gru = nn.GRUCell(mem_dim + time_dim + edge_dim, mem_dim)
        self.dst_gru = nn.GRUCell(mem_dim + time_dim + edge_dim, mem_dim)

    def forward(self, src_mem, dst_mem, dt_secs, edge_feat):
        """
        src_mem: [B, 32]
        dst_mem: [B, 32]
        dt_secs: [B, 1]
        edge_feat: [B, 8]
        Returns: risk_score [B, 1], new_src_mem [B, 32], new_dst_mem [B, 32]
        """
        t_enc = self.time_encoder(dt_secs) # [B, 16]
        concat_feat = torch.cat([src_mem, dst_mem, t_enc, edge_feat], dim=-1)
        risk_score = self.mlp(concat_feat)

        src_msg = torch.cat([dst_mem, t_enc, edge_feat], dim=-1)
        dst_msg = torch.cat([src_mem, t_enc, edge_feat], dim=-1)

        new_src = self.src_gru(src_msg, src_mem)
        new_dst = self.dst_gru(dst_msg, dst_mem)

        return risk_score, new_src, new_dst

FORMATS = ["ACH", "Wire", "Credit Card", "Cheque", "Cash", "Bitcoin", "Reinvestment"]

def format_one_hot(fmt_str):
    res = [0.0] * len(FORMATS)
    if fmt_str in FORMATS:
        res[FORMATS.index(fmt_str)] = 1.0
    return res

def load_ibm_sample(csv_path, max_rows=500_000):
    print(f"Loading up to {max_rows} rows from {csv_path}...")
    edges = []
    nodes = {}

    with open(csv_path, "r", encoding="utf-8") as f:
        reader = csv.reader(f)
        header = next(reader)

        for idx, row in enumerate(reader):
            if idx >= max_rows:
                break
            # Timestamp,From Bank,Account,To Bank,Account,Amount Received,Receiving Currency,Amount Paid,Payment Currency,Payment Format,Is Laundering
            ts_str = row[0] # 2022/09/01 00:20
            t_dt = dt.datetime.strptime(ts_str, "%Y/%m/%d %H:%M")
            ts = t_dt.timestamp()

            src_str = f"{row[1]}-{row[2]}"
            dst_str = f"{row[3]}-{row[4]}"
            if src_str == dst_str:
                continue # ignore self-transfers

            if src_str not in nodes:
                nodes[src_str] = len(nodes)
            if dst_str not in nodes:
                nodes[dst_str] = len(nodes)

            src_id = nodes[src_str]
            dst_id = nodes[dst_str]

            amt = float(row[7])
            log_amt = math.log1p(amt)
            fmt_oh = format_one_hot(row[9])
            edge_feat = [log_amt] + fmt_oh # 1 + 7 = 8 features
            is_laundering = 1.0 if row[10] == "1" else 0.0

            edges.append((ts, src_id, dst_id, edge_feat, is_laundering))

    print(f"Loaded {len(edges)} valid edges across {len(nodes)} distinct nodes.")
    return edges, len(nodes)

def train_and_export():
    csv_path = os.path.join("bench", "data", "HI-Small_Trans.csv")
    if not os.path.exists(csv_path):
        print(f"Dataset {csv_path} not found!")
        return

    edges, num_nodes = load_ibm_sample(csv_path, max_rows=1_200_000)

    # Sort strictly by timestamp
    edges.sort(key=lambda x: x[0])

    split_idx = int(len(edges) * 0.8)
    train_edges = edges[:split_idx]
    test_edges = edges[split_idx:]

    model = TGNScorerModel(mem_dim=32, edge_dim=8, time_dim=16, hidden_dim=64)
    optimizer = optim.Adam(model.parameters(), lr=0.003)

    # In-memory node states
    node_memory = torch.zeros((num_nodes, 32))
    last_update = torch.zeros((num_nodes, 1))

    print("Beginning continuous-time dynamic graph training...")
    batch_size = 1024
    epochs = 4

    for epoch in range(epochs):
        model.train()
        total_loss = 0.0
        pos_correct = 0
        total_pos = 0

        # Reset memory for each epoch
        node_memory.zero_()
        last_update.zero_()

        num_batches = len(train_edges) // batch_size
        start_t = time.time()

        for b in range(num_batches):
            b_edges = train_edges[b * batch_size : (b + 1) * batch_size]

            src_ids = [e[1] for e in b_edges]
            dst_ids = [e[2] for e in b_edges]
            times = [e[0] for e in b_edges]
            feats = [e[3] for e in b_edges]
            labels = torch.tensor([e[4] for e in b_edges], dtype=torch.float32).unsqueeze(1)

            t_tensor = torch.tensor(times, dtype=torch.float32).unsqueeze(1)
            src_last = last_update[src_ids]
            dst_last = last_update[dst_ids]
            dt_s = torch.clamp(t_tensor - torch.min(src_last, dst_last), min=0.0, max=864000.0)

            src_mem = node_memory[src_ids]
            dst_mem = node_memory[dst_ids]
            feat_tensor = torch.tensor(feats, dtype=torch.float32)

            optimizer.zero_grad()
            preds, new_src, new_dst = model(src_mem, dst_mem, dt_s, feat_tensor)

            # Class-weighted loss for extreme imbalance
            weights = torch.where(labels == 1.0, 300.0, 1.0)
            loss = nn.functional.binary_cross_entropy(preds, labels, weight=weights)
            loss.backward()
            optimizer.step()

            total_loss += loss.item()

            # Detach and update node memories
            node_memory[src_ids] = new_src.detach()
            node_memory[dst_ids] = new_dst.detach()
            last_update[src_ids] = t_tensor
            last_update[dst_ids] = t_tensor

            total_pos += int(torch.sum(labels).item())
            pos_correct += int(torch.sum((preds > 0.3) & (labels == 1.0)).item())

        elapsed = time.time() - start_t
        print(f"Epoch {epoch+1}/{epochs}: Loss = {total_loss/num_batches:.4f}, Pos Caught (>0.3) = {pos_correct}/{total_pos}, Time = {elapsed:.1f}s")

    # Evaluation on Test Split
    model.eval()
    test_pos_caught = 0
    test_pos_total = 0
    test_total = len(test_edges)

    with torch.no_grad():
        test_batches = len(test_edges) // batch_size
        for b in range(test_batches):
            b_edges = test_edges[b * batch_size : (b + 1) * batch_size]
            src_ids = [e[1] for e in b_edges]
            dst_ids = [e[2] for e in b_edges]
            times = [e[0] for e in b_edges]
            feats = [e[3] for e in b_edges]
            labels = torch.tensor([e[4] for e in b_edges], dtype=torch.float32).unsqueeze(1)

            t_tensor = torch.tensor(times, dtype=torch.float32).unsqueeze(1)
            src_last = last_update[src_ids]
            dst_last = last_update[dst_ids]
            dt_s = torch.clamp(t_tensor - torch.min(src_last, dst_last), min=0.0, max=864000.0)

            src_mem = node_memory[src_ids]
            dst_mem = node_memory[dst_ids]
            feat_tensor = torch.tensor(feats, dtype=torch.float32)

            preds, new_src, new_dst = model(src_mem, dst_mem, dt_s, feat_tensor)
            node_memory[src_ids] = new_src
            node_memory[dst_ids] = new_dst
            last_update[src_ids] = t_tensor
            last_update[dst_ids] = t_tensor

            test_pos_total += int(torch.sum(labels).item())
            test_pos_caught += int(torch.sum((preds > 0.5) & (labels == 1.0)).item())

    test_recall = (test_pos_caught / test_pos_total * 100) if test_pos_total > 0 else 0
    print(f"Test Set Evaluation: {test_pos_caught}/{test_pos_total} laundering txns caught ({test_recall:.1f}% recall).")

    # ONNX Export
    os.makedirs(os.path.join("ml", "models"), exist_ok=True)
    onnx_path = os.path.join("ml", "models", "tgn_scoring.onnx")
    print(f"Exporting model to ONNX: {onnx_path}...")

    dummy_src = torch.zeros((1, 32), dtype=torch.float32)
    dummy_dst = torch.zeros((1, 32), dtype=torch.float32)
    dummy_dt = torch.zeros((1, 1), dtype=torch.float32)
    dummy_feat = torch.zeros((1, 8), dtype=torch.float32)

    torch.onnx.export(
        model,
        (dummy_src, dummy_dst, dummy_dt, dummy_feat),
        onnx_path,
        export_params=True,
        opset_version=14,
        do_constant_folding=True,
        input_names=["src_mem", "dst_mem", "dt_secs", "edge_feat"],
        output_names=["risk_score", "new_src_mem", "new_dst_mem"],
        dynamic_axes={
            "src_mem": {0: "batch_size"},
            "dst_mem": {0: "batch_size"},
            "dt_secs": {0: "batch_size"},
            "edge_feat": {0: "batch_size"},
            "risk_score": {0: "batch_size"},
            "new_src_mem": {0: "batch_size"},
            "new_dst_mem": {0: "batch_size"},
        }
    )
    print(f"Model exported successfully. Size: {os.path.getsize(onnx_path)} bytes.")

if __name__ == "__main__":
    train_and_export()
