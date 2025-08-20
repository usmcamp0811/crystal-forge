from dataclasses import dataclass
from typing import Any, Dict


@dataclass
class CrystalForgeTestContext:
    """Test context containing VMs and configuration"""

    gitserver: Any
    server: Any
    agent: Any
    logger: Any
    system_info: Dict[str, str]
