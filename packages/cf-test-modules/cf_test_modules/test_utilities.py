"""
Utility functions for Crystal Forge tests
"""

def format_duration(seconds: float) -> str:
    """Format duration in seconds to human readable format"""
    if seconds < 60:
        return f"{seconds:.1f}s"
    elif seconds < 3600:
        return f"{seconds/60:.1f}m"
    else:
        return f"{seconds/3600:.1f}h"

def sanitize_hostname(hostname: str) -> str:
    """Sanitize hostname for use in file names"""
    return hostname.replace(".", "_").replace("-", "_")
