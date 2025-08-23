"""
Test-specific exceptions for Crystal Forge tests
"""

class AssertionFailedException(Exception):
    """Exception raised when a test assertion fails"""
    
    def __init__(self, test_name: str, reason: str, sql_query: str = None):
        self.test_name = test_name
        self.reason = reason
        self.sql_query = sql_query
        
        message = f"Test '{test_name}' failed: {reason}"
        if sql_query:
            message += f"\nSQL: {sql_query[:100]}..."
        
        super().__init__(message)
