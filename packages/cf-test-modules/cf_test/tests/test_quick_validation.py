#!/usr/bin/env python3
"""
Quick validation script for systems status view
"""
import json

from cf_test import CFTestClient, CFTestConfig


def main():
    """Run quick validation"""
    print("🔍 Quick Systems Status View Validation")
    print("=" * 40)

    try:
        config = CFTestConfig()
        client = CFTestClient(config)

        # Test 1: View exists
        print("1. Checking view exists...")
        result = client.execute_sql(
            "SELECT COUNT(*) as count FROM view_systems_status_table"
        )
        system_count = result[0]["count"]
        print(f"   ✅ Found {system_count} systems in view")

        # Test 2: View structure
        print("2. Checking view structure...")
        columns = client.execute_sql(
            """
            SELECT column_name 
            FROM information_schema.columns 
            WHERE table_name = 'view_systems_status_table'
            ORDER BY ordinal_position
        """
        )

        column_names = [c["column_name"] for c in columns]
        required = [
            "hostname",
            "connectivity_status",
            "update_status",
            "overall_status",
        ]

        for col in required:
            if col in column_names:
                print(f"   ✅ Column '{col}' exists")
            else:
                print(f"   ❌ Column '{col}' missing")

        # Test 3: Sample data
        print("3. Sample data...")
        sample = client.execute_sql(
            """
            SELECT hostname, connectivity_status, update_status, overall_status
            FROM view_systems_status_table 
            LIMIT 5
        """
        )

        if sample:
            for row in sample:
                print(f"   📍 {row['hostname']}: {row['overall_status']}")
        else:
            print("   ℹ️  No systems found")

        # Test 4: Status distribution
        print("4. Status distribution...")
        stats = client.execute_sql(
            """
            SELECT 
                overall_status,
                COUNT(*) as count
            FROM view_systems_status_table
            GROUP BY overall_status
            ORDER BY count DESC
        """
        )

        for stat in stats:
            print(f"   📊 {stat['overall_status']}: {stat['count']}")

        print("\n✅ Quick validation completed successfully!")

    except Exception as e:
        print(f"\n❌ Validation failed: {e}")
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main())
