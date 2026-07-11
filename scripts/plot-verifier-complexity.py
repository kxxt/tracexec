#!/usr/bin/env python3
"""Plot tracexec eBPF verifier complexity results.

The input is one or more JSON files produced by tracexec-verifier-complexity,
or directories containing those files. The script writes heatmaps, max-by-build
charts, and a small Markdown/CSV summary.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any


DEFAULT_INPUT = Path("verifier-complexity")
DEFAULT_OUTPUT = Path("verifier-complexity-plots")
DEFAULT_METRICS = (
    "insns_processed",
    "verification_time_microseconds",
    "peak_states",
    "stack_depth",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Plot verifier complexity JSON files produced by UKCI complexity runs."
    )
    parser.add_argument(
        "inputs",
        nargs="*",
        type=Path,
        default=[DEFAULT_INPUT],
        help="JSON files or directories containing JSON files (default: verifier-complexity)",
    )
    parser.add_argument(
        "-o",
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="directory for charts and summaries (default: verifier-complexity-plots)",
    )
    parser.add_argument(
        "--metrics",
        default=",".join(DEFAULT_METRICS),
        help="comma-separated metrics to plot",
    )
    parser.add_argument(
        "--top",
        type=int,
        default=25,
        help="number of load/program rows to include in each heatmap (default: 25)",
    )
    parser.add_argument(
        "--format",
        choices=("png", "pdf", "svg"),
        default="png",
        help="plot output format (default: png)",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=160,
        help="raster plot DPI for PNG output (default: 160)",
    )
    parser.add_argument(
        "--log-scale",
        action="store_true",
        help="use logarithmic color scale for heatmaps",
    )
    parser.add_argument(
        "--no-plots",
        action="store_true",
        help="write only summary files; useful when matplotlib is unavailable",
    )
    return parser.parse_args()


def find_json_files(inputs: Iterable[Path]) -> list[Path]:
    files: list[Path] = []
    for input_path in inputs:
        if input_path.is_dir():
            files.extend(
                path
                for path in sorted(input_path.rglob("*.json"))
                if not path.name.endswith(".tmp")
            )
        elif input_path.is_file():
            files.append(input_path)
        else:
            raise FileNotFoundError(f"input does not exist: {input_path}")

    if not files:
        raise FileNotFoundError("no JSON files found in the requested inputs")
    return files


def load_records(files: Iterable[Path]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for path in files:
        with path.open(encoding="utf-8") as handle:
            data = json.load(handle)

        if isinstance(data, dict) and isinstance(data.get("records"), list):
            file_records = data["records"]
        elif isinstance(data, list):
            file_records = data
        else:
            raise ValueError(f"{path}: expected a JSON array or an object with a records array")

        for index, record in enumerate(file_records):
            if not isinstance(record, dict):
                raise ValueError(f"{path}: record {index} is not an object")
            for field in ("build", "load", "program"):
                if field not in record:
                    raise ValueError(f"{path}: record {index} is missing {field!r}")
            loaded = dict(record)
            loaded["_source_file"] = str(path)
            records.append(loaded)

    if not records:
        raise ValueError("no verifier complexity records found")
    return records


def parse_metrics(value: str) -> list[str]:
    metrics = [metric.strip() for metric in value.split(",") if metric.strip()]
    if not metrics:
        raise ValueError("at least one metric is required")
    return metrics


def record_metric(record: dict[str, Any], metric: str) -> float | None:
    value = record.get(metric)

    result = number(value)
    return result if result is not None and math.isfinite(result) else None


def number(value: Any) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return float(value)
        except ValueError:
            return None
    return None


def load_program_key(record: dict[str, Any]) -> str:
    return f"{record['load']}/{record['program']}"


def natural_key(value: str) -> list[int | str]:
    return [
        int(part) if part.isdigit() else part
        for part in re.split(r"(\d+)", value)
        if part != ""
    ]


def slug(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", value).strip("-").lower()


def metric_label(metric: str) -> str:
    return metric.replace("_", " ")


def format_number(value: float | None) -> str:
    if value is None or math.isnan(value):
        return ""
    if value.is_integer():
        return f"{int(value):,}"
    return f"{value:,.2f}"


def markdown_cell(value: Any) -> str:
    return str(value).replace("|", "\\|")


def records_for_metric(
    records: Iterable[dict[str, Any]], metric: str
) -> list[tuple[float, dict[str, Any]]]:
    rows = []
    for record in records:
        value = record_metric(record, metric)
        if value is not None:
            rows.append((value, record))
    rows.sort(key=lambda row: row[0], reverse=True)
    return rows


def write_summary(
    output_dir: Path,
    records: list[dict[str, Any]],
    files: list[Path],
    metrics: list[str],
    top: int,
) -> None:
    builds = sorted({str(record["build"]) for record in records}, key=natural_key)
    load_programs = sorted({load_program_key(record) for record in records}, key=natural_key)

    summary_path = output_dir / "summary.md"
    with summary_path.open("w", encoding="utf-8") as handle:
        handle.write("# Verifier Complexity Summary\n\n")
        handle.write(f"- Records: {len(records):,}\n")
        handle.write(f"- Input files: {len(files):,}\n")
        handle.write(f"- Builds: {len(builds):,}\n")
        handle.write(f"- Load/program pairs: {len(load_programs):,}\n\n")

        handle.write("## Builds\n\n")
        for build in builds:
            releases = sorted(
                {
                    str(record.get("kernel_release", "unknown"))
                    for record in records
                    if record["build"] == build
                },
                key=natural_key,
            )
            release_text = ", ".join(releases)
            handle.write(f"- `{build}` ({release_text})\n")

        for metric in metrics:
            rows = records_for_metric(records, metric)
            if not rows:
                continue

            handle.write(f"\n## Top {metric_label(metric)}\n\n")
            handle.write("| value | build | load | program | kernel |\n")
            handle.write("| ---: | --- | --- | --- | --- |\n")
            for value, record in rows[:top]:
                handle.write(
                    "| "
                    f"{format_number(value)} | "
                    f"`{markdown_cell(record['build'])}` | "
                    f"`{markdown_cell(record['load'])}` | "
                    f"`{markdown_cell(record['program'])}` | "
                    f"`{markdown_cell(record.get('kernel_release', ''))}` |\n"
                )

    csv_path = output_dir / "top-records.csv"
    with csv_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "metric",
                "rank",
                "value",
                "build",
                "load",
                "program",
                "kernel_release",
                "arch",
                "source_file",
            ],
        )
        writer.writeheader()
        for metric in metrics:
            for rank, (value, record) in enumerate(
                records_for_metric(records, metric)[:top], start=1
            ):
                writer.writerow(
                    {
                        "metric": metric,
                        "rank": rank,
                        "value": int(value) if value.is_integer() else value,
                        "build": record["build"],
                        "load": record["load"],
                        "program": record["program"],
                        "kernel_release": record.get("kernel_release", ""),
                        "arch": record.get("arch", ""),
                        "source_file": record.get("_source_file", ""),
                    }
                )


def require_plotting_modules() -> tuple[Any, Any, Any]:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        import numpy as np
        from matplotlib import colors
    except ImportError as error:
        raise RuntimeError(
            "matplotlib is required to render plots. "
            "Run with Nix using: nix run .#plot-verifier-complexity -- verifier-complexity"
        ) from error

    return plt, np, colors


def metric_matrix(
    records: list[dict[str, Any]],
    metric: str,
    top: int,
) -> tuple[list[str], list[str], list[list[float]]]:
    cell_values: dict[tuple[str, str], float] = {}
    max_by_key: dict[str, float] = {}
    builds = sorted({str(record["build"]) for record in records}, key=natural_key)

    for record in records:
        value = record_metric(record, metric)
        if value is None:
            continue

        key = load_program_key(record)
        build = str(record["build"])
        cell = (key, build)
        cell_values[cell] = max(value, cell_values.get(cell, float("-inf")))
        max_by_key[key] = max(value, max_by_key.get(key, float("-inf")))

    keys = [
        key
        for key, _ in sorted(
            max_by_key.items(), key=lambda item: (-item[1], natural_key(item[0]))
        )[:top]
    ]
    matrix = [
        [cell_values.get((key, build), math.nan) for build in builds]
        for key in keys
    ]
    return keys, builds, matrix


def plot_heatmap(
    plt: Any,
    np: Any,
    colors: Any,
    output_dir: Path,
    records: list[dict[str, Any]],
    metric: str,
    top: int,
    output_format: str,
    dpi: int,
    log_scale: bool,
) -> Path | None:
    keys, builds, matrix = metric_matrix(records, metric, top)
    if not keys:
        return None

    values = np.array(matrix, dtype=float)
    masked_values = np.ma.masked_invalid(values)
    cmap = plt.get_cmap("viridis").copy()
    cmap.set_bad("#f2f2f2")

    positive_values = values[np.isfinite(values) & (values > 0)]
    norm = None
    if log_scale:
        masked_values = np.ma.masked_where(values <= 0, masked_values)
        if positive_values.size > 0:
            min_value = float(positive_values.min())
            max_value = float(positive_values.max())
            if min_value < max_value:
                norm = colors.LogNorm(vmin=min_value, vmax=max_value)

    width = max(8.0, 0.55 * len(builds) + 3.0)
    height = max(5.0, 0.34 * len(keys) + 2.0)
    fig, ax = plt.subplots(figsize=(width, height))
    image = ax.imshow(masked_values, aspect="auto", cmap=cmap, norm=norm)

    ax.set_title(f"Top {len(keys)} {metric_label(metric)} values by load/program")
    ax.set_xlabel("build")
    ax.set_ylabel("load/program")
    ax.set_xticks(range(len(builds)), labels=builds, rotation=45, ha="right")
    ax.set_yticks(range(len(keys)), labels=keys)
    ax.tick_params(axis="both", labelsize=8)
    fig.colorbar(image, ax=ax, label=metric_label(metric))
    fig.tight_layout()

    path = output_dir / f"{slug(metric)}-heatmap.{output_format}"
    fig.savefig(path, dpi=dpi)
    plt.close(fig)
    return path


def plot_max_by_build(
    plt: Any,
    output_dir: Path,
    records: list[dict[str, Any]],
    metric: str,
    output_format: str,
    dpi: int,
) -> Path | None:
    max_by_build: dict[str, float] = {}
    for record in records:
        value = record_metric(record, metric)
        if value is None:
            continue
        build = str(record["build"])
        max_by_build[build] = max(value, max_by_build.get(build, float("-inf")))

    if not max_by_build:
        return None

    builds = sorted(max_by_build, key=natural_key)
    values = [max_by_build[build] for build in builds]

    width = max(8.0, 0.55 * len(builds) + 3.0)
    fig, ax = plt.subplots(figsize=(width, 5.0))
    ax.bar(range(len(builds)), values, color="#3b728f")
    ax.set_title(f"Maximum {metric_label(metric)} by build")
    ax.set_xlabel("build")
    ax.set_ylabel(metric_label(metric))
    ax.set_xticks(range(len(builds)), labels=builds, rotation=45, ha="right")
    ax.tick_params(axis="x", labelsize=8)
    ax.yaxis.grid(True, linestyle=":", alpha=0.45)
    ax.set_axisbelow(True)
    fig.tight_layout()

    path = output_dir / f"{slug(metric)}-max-by-build.{output_format}"
    fig.savefig(path, dpi=dpi)
    plt.close(fig)
    return path


def main() -> int:
    args = parse_args()

    try:
        metrics = parse_metrics(args.metrics)
        if args.top < 1:
            raise ValueError("--top must be at least 1")

        files = find_json_files(args.inputs)
        records = load_records(files)
        args.output_dir.mkdir(parents=True, exist_ok=True)
        write_summary(args.output_dir, records, files, metrics, args.top)

        written_plots: list[Path] = []
        if not args.no_plots:
            plt, np, colors = require_plotting_modules()
            for metric in metrics:
                heatmap = plot_heatmap(
                    plt,
                    np,
                    colors,
                    args.output_dir,
                    records,
                    metric,
                    args.top,
                    args.format,
                    args.dpi,
                    args.log_scale,
                )
                max_by_build = plot_max_by_build(
                    plt,
                    args.output_dir,
                    records,
                    metric,
                    args.format,
                    args.dpi,
                )
                written_plots.extend(path for path in (heatmap, max_by_build) if path)

        print(f"wrote summary: {args.output_dir / 'summary.md'}")
        print(f"wrote CSV: {args.output_dir / 'top-records.csv'}")
        if written_plots:
            print(f"wrote plots: {len(written_plots)}")
        elif args.no_plots:
            print("plots skipped")
        else:
            print("no metric values were available to plot")
        return 0
    except (OSError, ValueError, json.JSONDecodeError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
