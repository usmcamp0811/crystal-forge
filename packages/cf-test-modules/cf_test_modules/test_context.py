import os
from dataclasses import dataclass
from typing import Any, Dict, Optional


@dataclass
class CrystalForgeTestContext:
    """Test context containing VMs and configuration"""

    gitserver: Any
    server: Any
    agent: Any
    logger: Any
    system_info: Dict[str, str]
    exit_on_failure: bool = False

    # Server (derived from env if None)
    cf_server_host: Optional[str] = None
    cf_server_port: Optional[int] = None

    # Database (derived from env if None)
    db_host: Optional[str] = None
    db_port: Optional[int] = None
    db_user: Optional[str] = None
    db_password: Optional[str] = None
    db_name: Optional[str] = None

    def __post_init__(self) -> None:
        # Server
        if self.cf_server_host is None:
            self.cf_server_host = (
                os.getenv("CF_SERVER_HOST")
                or os.getenv("cf_server_host")
                or "127.0.0.1"
            )
        if self.cf_server_port is None:
            port_val = (
                os.getenv("CF_SERVER_PORT") or os.getenv("cf_server_port") or "3445"
            )
            try:
                self.cf_server_port = int(port_val)
            except ValueError:
                self.cf_server_port = 3445

        # Database
        if self.db_host is None:
            self.db_host = os.getenv("DB_HOST") or os.getenv("db_host") or "127.0.0.1"
        if self.db_port is None:
            db_port_val = os.getenv("DB_PORT") or os.getenv("db_port") or "3042"
            try:
                self.db_port = int(db_port_val)
            except ValueError:
                self.db_port = 3042
        if self.db_user is None:
            self.db_user = (
                os.getenv("DB_USER") or os.getenv("db_user") or "crystal_forge"
            )
        if self.db_password is None:
            self.db_password = (
                os.getenv("DB_PASSWORD") or os.getenv("db_password") or "password"
            )
        if self.db_name is None:
            self.db_name = (
                os.getenv("DB_NAME") or os.getenv("db_name") or "crystal_forge"
            )

        # Surface into system_info for backward compatibility
        self.system_info.setdefault("cf_server_host", self.cf_server_host)
        self.system_info.setdefault("cf_server_port", str(self.cf_server_port))
        self.system_info.setdefault("db_host", self.db_host)
        self.system_info.setdefault("db_port", str(self.db_port))
        self.system_info.setdefault("db_user", self.db_user)
        self.system_info.setdefault("db_name", self.db_name)
