#!/usr/bin/env python3
"""
Campaign YAML parser and validator for SpinSAT Optuna tuning.

Loads a campaign YAML file, validates its schema, resolves instance
glob patterns, and returns a CampaignConfig ready for optuna_tune.py.

Usage (as library):
    from campaign_config import load_campaign
    config = load_campaign("campaigns/tune_ode_full.yaml")

Usage (standalone validation):
    python3 scripts/campaign_config.py campaigns/tune_ode_full.yaml
"""

import glob
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

import yaml


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class SearchParam:
    """One entry in the search_space section."""
    name: str
    type: str  # "float", "int", "categorical"
    low: Optional[float] = None
    high: Optional[float] = None
    log: bool = False
    choices: Optional[list] = None
    condition: Optional[dict] = None  # e.g. {"auto_zeta": false}


@dataclass
class SamplerConfig:
    type: str = "TPE"
    seed: Optional[int] = None


@dataclass
class PrunerConfig:
    type: str = "SuccessiveHalving"
    min_resource: int = 5
    reduction_factor: int = 3
    n_startup_trials: int = 5
    n_warmup_steps: int = 30


@dataclass
class StorageConfig:
    type: str = "sqlite"
    path: str = ""
    url: str = ""  # PostgreSQL connection URL (for type=postgresql)


@dataclass
class ValidationConfig:
    timeout_s: int = 5000
    seeds: list = field(default_factory=lambda: [42, 137, 271, 404, 999])
    record_to_db: bool = True


@dataclass
class CampaignConfig:
    """Fully validated campaign configuration."""
    study_name: str

    # Objective
    metric: str
    direction: str
    seeds: list
    timeout_s: int

    # Instances
    instance_patterns: list
    max_instances: Optional[int]
    resolved_instances: list  # populated after glob resolution

    # Search space
    search_space: list  # list of SearchParam

    # Budget
    n_trials: int
    max_wall_hours: Optional[float]

    # Components
    sampler: SamplerConfig
    pruner: PrunerConfig
    storage: StorageConfig
    validation: Optional[ValidationConfig]


# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------

class CampaignError(Exception):
    """Raised when a campaign YAML is invalid."""


_VALID_PARAM_TYPES = {"float", "int", "categorical"}
_VALID_METRICS = {"par2"}
_VALID_DIRECTIONS = {"minimize", "maximize"}
_VALID_SAMPLER_TYPES = {"TPE", "Random", "Grid", "CmaEs"}
_VALID_PRUNER_TYPES = {"SuccessiveHalving", "Hyperband", "Median", "MedianPruner", "NopPruner"}
_VALID_STORAGE_TYPES = {"sqlite", "postgresql"}

# Known solver parameter names (validated but not enforced — unknown names
# are allowed so the config stays forward-compatible with new solver flags).
_KNOWN_PARAMS = {
    "alpha_initial", "alpha_up_mult", "alpha_down_mult", "alpha_interval",
    "beta", "gamma", "delta", "epsilon", "zeta", "auto_zeta",
    "strategy", "method",  # "method" is an alias for "strategy"
    "no_restart", "restart_mode", "xl_decay", "restart_noise",
    "preprocess",
}


def _require(d: dict, key: str, context: str) -> Any:
    if key not in d:
        raise CampaignError(f"Missing required field '{key}' in {context}")
    return d[key]


def _coerce_number(value: Any, context: str) -> float:
    """Coerce a value to float, handling YAML string edge cases like '1e-3'."""
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return float(value)
        except ValueError:
            pass
    raise CampaignError(f"{context}: expected a number, got {value!r}")


def _validate_search_param(name: str, spec: dict) -> SearchParam:
    """Validate a single search_space entry."""
    ptype = _require(spec, "type", f"search_space.{name}")
    if ptype not in _VALID_PARAM_TYPES:
        raise CampaignError(
            f"search_space.{name}.type must be one of {_VALID_PARAM_TYPES}, got '{ptype}'"
        )

    param = SearchParam(name=name, type=ptype)

    if ptype in ("float", "int"):
        raw_low = _require(spec, "low", f"search_space.{name}")
        raw_high = _require(spec, "high", f"search_space.{name}")
        param.low = _coerce_number(raw_low, f"search_space.{name}.low")
        param.high = _coerce_number(raw_high, f"search_space.{name}.high")
        if param.low >= param.high:
            raise CampaignError(
                f"search_space.{name}: low ({param.low}) must be < high ({param.high})"
            )
        param.log = spec.get("log", False)
        if param.log and param.low <= 0:
            raise CampaignError(
                f"search_space.{name}: log scale requires low > 0, got {param.low}"
            )
    elif ptype == "categorical":
        param.choices = _require(spec, "choices", f"search_space.{name}")
        if not isinstance(param.choices, list) or len(param.choices) == 0:
            raise CampaignError(
                f"search_space.{name}.choices must be a non-empty list"
            )

    if "condition" in spec:
        cond = spec["condition"]
        if not isinstance(cond, dict) or len(cond) != 1:
            raise CampaignError(
                f"search_space.{name}.condition must be a dict with exactly one key"
            )
        param.condition = cond

    return param


def _resolve_instances(patterns: list, max_instances: Optional[int],
                       base_dir: Path) -> list:
    """Resolve glob patterns to sorted, deduplicated file paths."""
    found = set()
    for pattern in patterns:
        # Resolve relative patterns against the project root
        if not Path(pattern).is_absolute():
            pattern = str(base_dir / pattern)
        matches = glob.glob(pattern)
        found.update(matches)

    instances = sorted(found)
    if max_instances is not None:
        instances = instances[:max_instances]
    return instances


# ---------------------------------------------------------------------------
# Main loader
# ---------------------------------------------------------------------------

def load_campaign(yaml_path: str, resolve_instances: bool = True) -> CampaignConfig:
    """Load and validate a campaign YAML file.

    Args:
        yaml_path: Path to the campaign YAML file.
        resolve_instances: If True, resolve glob patterns to actual files.
            Set False for pure validation without filesystem access.

    Returns:
        A validated CampaignConfig.

    Raises:
        CampaignError: If the YAML is invalid or missing required fields.
        FileNotFoundError: If the YAML file doesn't exist.
    """
    path = Path(yaml_path)
    if not path.exists():
        raise FileNotFoundError(f"Campaign file not found: {yaml_path}")

    with open(path) as f:
        raw = yaml.safe_load(f)

    if not isinstance(raw, dict):
        raise CampaignError("Campaign YAML must be a mapping at top level")

    # --- study_name ---
    study_name = _require(raw, "study_name", "root")
    if not isinstance(study_name, str) or not study_name.strip():
        raise CampaignError("study_name must be a non-empty string")

    # --- objective ---
    obj = _require(raw, "objective", "root")
    metric = obj.get("metric", "par2")
    if metric not in _VALID_METRICS:
        raise CampaignError(f"objective.metric must be one of {_VALID_METRICS}")
    direction = obj.get("direction", "minimize")
    if direction not in _VALID_DIRECTIONS:
        raise CampaignError(f"objective.direction must be one of {_VALID_DIRECTIONS}")
    seeds = _require(obj, "seeds", "objective")
    if not isinstance(seeds, list) or not all(isinstance(s, int) for s in seeds):
        raise CampaignError("objective.seeds must be a list of integers")
    timeout_s = _require(obj, "timeout_s", "objective")
    if not isinstance(timeout_s, (int, float)) or timeout_s <= 0:
        raise CampaignError("objective.timeout_s must be a positive number")
    timeout_s = int(timeout_s)

    # --- instances ---
    inst = _require(raw, "instances", "root")
    patterns = _require(inst, "patterns", "instances")
    if not isinstance(patterns, list) or len(patterns) == 0:
        raise CampaignError("instances.patterns must be a non-empty list of glob strings")
    max_inst = inst.get("max_instances")
    if max_inst is not None:
        if not isinstance(max_inst, int) or max_inst <= 0:
            raise CampaignError("instances.max_instances must be a positive integer")

    # Resolve instance globs
    project_root = Path(__file__).parent.parent
    if resolve_instances:
        resolved = _resolve_instances(patterns, max_inst, project_root)
    else:
        resolved = []

    # --- search_space ---
    space_raw = _require(raw, "search_space", "root")
    if not isinstance(space_raw, dict) or len(space_raw) == 0:
        raise CampaignError("search_space must be a non-empty mapping")
    search_space = []
    for name, spec in space_raw.items():
        search_space.append(_validate_search_param(name, spec))

    # Validate conditional references: condition keys must exist in search_space
    space_names = {p.name for p in search_space}
    for p in search_space:
        if p.condition:
            for cond_key in p.condition:
                if cond_key not in space_names:
                    raise CampaignError(
                        f"search_space.{p.name}.condition references "
                        f"unknown param '{cond_key}'"
                    )

    # --- budget ---
    budget = _require(raw, "budget", "root")
    n_trials = _require(budget, "n_trials", "budget")
    if not isinstance(n_trials, int) or n_trials <= 0:
        raise CampaignError("budget.n_trials must be a positive integer")
    max_wall_hours = budget.get("max_wall_hours")

    # --- sampler ---
    sampler_raw = raw.get("sampler", {})
    sampler = SamplerConfig(
        type=sampler_raw.get("type", "TPE"),
        seed=sampler_raw.get("seed"),
    )
    if sampler.type not in _VALID_SAMPLER_TYPES:
        raise CampaignError(
            f"sampler.type must be one of {_VALID_SAMPLER_TYPES}, got '{sampler.type}'"
        )

    # --- pruner ---
    pruner_raw = raw.get("pruner", {})
    pruner = PrunerConfig(
        type=pruner_raw.get("type", "SuccessiveHalving"),
        min_resource=pruner_raw.get("min_resource", 5),
        reduction_factor=pruner_raw.get("reduction_factor", 3),
        n_startup_trials=pruner_raw.get("n_startup_trials", 5),
        n_warmup_steps=pruner_raw.get("n_warmup_steps", 30),
    )
    if pruner.type not in _VALID_PRUNER_TYPES:
        raise CampaignError(
            f"pruner.type must be one of {_VALID_PRUNER_TYPES}, got '{pruner.type}'"
        )

    # --- storage ---
    stor_raw = raw.get("storage", {})
    storage = StorageConfig(
        type=stor_raw.get("type", "sqlite"),
        path=stor_raw.get("path", f"optuna_studies/{study_name}.db"),
        url=stor_raw.get("url", ""),
    )
    if storage.type not in _VALID_STORAGE_TYPES:
        raise CampaignError(
            f"storage.type must be one of {_VALID_STORAGE_TYPES}, got '{storage.type}'"
        )

    # --- validation (optional) ---
    val_raw = raw.get("validation")
    validation = None
    if val_raw:
        validation = ValidationConfig(
            timeout_s=val_raw.get("timeout_s", 5000),
            seeds=val_raw.get("seeds", [42, 137, 271, 404, 999]),
            record_to_db=val_raw.get("record_to_db", True),
        )

    return CampaignConfig(
        study_name=study_name,
        metric=metric,
        direction=direction,
        seeds=seeds,
        timeout_s=timeout_s,
        instance_patterns=patterns,
        max_instances=max_inst,
        resolved_instances=resolved,
        search_space=search_space,
        n_trials=n_trials,
        max_wall_hours=max_wall_hours,
        sampler=sampler,
        pruner=pruner,
        storage=storage,
        validation=validation,
    )


def print_summary(config: CampaignConfig) -> None:
    """Print a human-readable summary of the campaign config."""
    print(f"Study: {config.study_name}")
    print(f"Objective: {config.direction} {config.metric}")
    print(f"Seeds: {config.seeds}")
    print(f"Timeout: {config.timeout_s}s")
    print(f"Instances: {len(config.resolved_instances)} files "
          f"(from {len(config.instance_patterns)} pattern(s), "
          f"max={config.max_instances})")
    print(f"Search space: {len(config.search_space)} parameters")
    for p in config.search_space:
        if p.type == "categorical":
            desc = f"  {p.name}: categorical {p.choices}"
        else:
            log_str = " (log)" if p.log else ""
            desc = f"  {p.name}: {p.type} [{p.low}, {p.high}]{log_str}"
        if p.condition:
            desc += f" | condition: {p.condition}"
        print(desc)
    print(f"Budget: {config.n_trials} trials"
          + (f", {config.max_wall_hours}h max" if config.max_wall_hours else ""))
    print(f"Sampler: {config.sampler.type} (seed={config.sampler.seed})")
    print(f"Pruner: {config.pruner.type} "
          f"(min_resource={config.pruner.min_resource}, "
          f"reduction_factor={config.pruner.reduction_factor})")
    if config.storage.type == "postgresql":
        # Mask password in URL for display
        url = config.storage.url
        if "@" in url:
            pre, post = url.split("@", 1)
            if ":" in pre:
                scheme_user = pre.rsplit(":", 1)[0]
                url = f"{scheme_user}:***@{post}"
        print(f"Storage: {config.storage.type} → {url}")
    else:
        print(f"Storage: {config.storage.type} → {config.storage.path}")
    if config.validation:
        print(f"Validation: {config.validation.timeout_s}s timeout, "
              f"seeds={config.validation.seeds}, "
              f"record={config.validation.record_to_db}")

    # Cost estimate
    trials_per_instance = len(config.seeds) * config.timeout_s
    total_worst = config.n_trials * len(config.resolved_instances) * trials_per_instance
    print(f"\nWorst-case compute: {total_worst / 3600:.1f} CPU-hours "
          f"({config.n_trials} trials × {len(config.resolved_instances)} instances "
          f"× {len(config.seeds)} seeds × {config.timeout_s}s)")


# ---------------------------------------------------------------------------
# CLI entry point — standalone validation
# ---------------------------------------------------------------------------

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 scripts/campaign_config.py <campaign.yaml>", file=sys.stderr)
        sys.exit(1)

    yaml_path = sys.argv[1]
    try:
        config = load_campaign(yaml_path)
        print_summary(config)
        if not config.resolved_instances:
            print("\n⚠  No instances matched the glob patterns.", file=sys.stderr)
            sys.exit(1)
        print("\n✓ Campaign config is valid.")
    except (CampaignError, FileNotFoundError) as e:
        print(f"✗ {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
